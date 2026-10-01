//! PORTS: ghostty src/input/Binding.zig — `Set`, the trigger -> action map, with its hash-map
//! contexts, leaf/value types and `formatEntries`.

use super::*;

// ===========================================================================
// Set
// ===========================================================================

// PORT NOTE: the reference `Set` uses `std.ArrayHashMapUnmanaged(K, V, Context,
// store_hash=true)` for both the forward (`Trigger -> Value`) and reverse
// (`Action -> Trigger`) maps, where `Context` defines `hash` (truncated wyhash)
// and `eql` (`bindingSetEqual`: `Trigger.foldedEqual` / `Action.equal`). It is
// INSERTION-ORDERED with `getOrPut`/`swapRemove`/`swapRemoveAt`/`getEntry`.
//
// rustty-zigstd's `AutoArrayHashMap` requires `K: Eq + Hash` (Rust std hashing),
// which CANNOT express the folded-equality / truncated-wyhash contexts, so the
// `Set`'s two maps are implemented here as `CtxArrayHashMap<K, V>` — an
// insertion-ordered vector of entries plus a parallel stored-hash array, with all
// lookups going through a `MapContext` (exactly Zig's `Context`). This is the
// faithful analog of `ArrayHashMapUnmanaged(..., store_hash=true)`.

/// The hash map context for the set. This defines how the hash map
/// gets the hash key and checks for equality.
//
// PORT NOTE: mirrors Zig's `Set.Context(comptime KeyType)`.
trait MapContext<K> {
    fn ctx_hash(k: &K) -> u32;
    fn ctx_eql(a: &K, b: &K) -> bool;
}

/// Context for `Trigger` keys (forward map): folded equality.
struct TriggerCtx;
impl MapContext<Trigger> for TriggerCtx {
    fn ctx_hash(k: &Trigger) -> u32 {
        // This seems crazy at first glance but this is also how
        // the Zig standard library handles hashing for array
        // hash maps!
        // return @truncate(k.hash());
        k.hash() as u32
    }
    fn ctx_eql(a: &Trigger, b: &Trigger) -> bool {
        // return a.bindingSetEqual(b);  (Trigger.bindingSetEqual = foldedEqual)
        a.folded_equal(*b)
    }
}

/// Context for `Action` keys (reverse map): structural equality.
struct ActionCtx;
impl MapContext<Action> for ActionCtx {
    fn ctx_hash(k: &Action) -> u32 {
        k.hash() as u32
    }
    fn ctx_eql(a: &Action, b: &Action) -> bool {
        // Action.bindingSetEqual = equal
        a.equal(b)
    }
}

/// Insertion-ordered hash map with an explicit context (faithful port of
/// `std.ArrayHashMapUnmanaged(K, V, Context, store_hash=true)`).
//
// PORT NOTE: Clone/Debug/PartialEq are impl'd manually (below) because the
// `PhantomData<C>` marker carries a `MapContext` bound that need not itself be
// Clone/Debug/PartialEq.
struct CtxArrayHashMap<K, V, C: MapContext<K>> {
    /// Entries in insertion order: (key, stored_hash, value).
    entries: Vec<(K, u32, V)>,
    _ctx: std::marker::PhantomData<C>,
}

impl<K: Clone, V: Clone, C: MapContext<K>> Clone for CtxArrayHashMap<K, V, C> {
    fn clone(&self) -> Self {
        CtxArrayHashMap {
            entries: self.entries.clone(),
            _ctx: std::marker::PhantomData,
        }
    }
}

impl<K: std::fmt::Debug, V: std::fmt::Debug, C: MapContext<K>> std::fmt::Debug
    for CtxArrayHashMap<K, V, C>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CtxArrayHashMap")
            .field("entries", &self.entries)
            .finish()
    }
}

impl<K: PartialEq, V: PartialEq, C: MapContext<K>> PartialEq for CtxArrayHashMap<K, V, C> {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}

impl<K: Clone, V, C: MapContext<K>> CtxArrayHashMap<K, V, C> {
    fn new() -> Self {
        CtxArrayHashMap {
            entries: Vec::new(),
            _ctx: std::marker::PhantomData,
        }
    }

    fn count(&self) -> usize {
        self.entries.len()
    }

    /// Find the entry index for `key` via context hash+eql.
    fn find(&self, key: &K) -> Option<usize> {
        let h = C::ctx_hash(key);
        self.entries
            .iter()
            .position(|(k, kh, _)| *kh == h && C::ctx_eql(k, key))
    }

    /// `getEntry`: returns the index of the entry, if present.
    fn get_index(&self, key: &K) -> Option<usize> {
        self.find(key)
    }

    /// `get(key)` returning a value reference.
    fn get(&self, key: &K) -> Option<&V> {
        self.find(key).map(|i| &self.entries[i].2)
    }

    /// `getOrPut(key)`: returns (index, found_existing). New entries are
    /// appended with `make` for the value.
    fn get_or_put_with<F: FnOnce() -> V>(&mut self, key: K, make: F) -> (usize, bool) {
        if let Some(i) = self.find(&key) {
            (i, true)
        } else {
            let h = C::ctx_hash(&key);
            let i = self.entries.len();
            self.entries.push((key, h, make()));
            (i, false)
        }
    }

    /// `put(key, value)`: insert or overwrite (position preserved).
    fn put(&mut self, key: K, value: V) {
        if let Some(i) = self.find(&key) {
            self.entries[i].2 = value;
        } else {
            let h = C::ctx_hash(&key);
            self.entries.push((key, h, value));
        }
    }

    /// `swapRemove(key)`: remove by swapping the LAST entry into its slot.
    fn swap_remove(&mut self, key: &K) -> Option<V> {
        let i = self.find(key)?;
        Some(self.entries.swap_remove(i).2)
    }

    /// `swapRemoveAt(index)`.
    fn swap_remove_at(&mut self, index: usize) -> V {
        self.entries.swap_remove(index).2
    }

    fn key_at(&self, index: usize) -> &K {
        &self.entries[index].0
    }
    fn value_at(&self, index: usize) -> &V {
        &self.entries[index].2
    }
    fn value_at_mut(&mut self, index: usize) -> &mut V {
        &mut self.entries[index].2
    }
}

/// Leaf node of a set is an action to trigger. This is a "leaf" compared
/// to the inner nodes which are "leaders" for sequences.
#[derive(Clone, Debug, PartialEq)]
pub struct Leaf {
    pub action: Action,
    pub flags: Flags,
}

impl Leaf {
    pub fn clone_in(&self) -> Leaf {
        Leaf {
            action: self.action.clone_action(),
            flags: self.flags,
        }
    }

    pub fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.action.hash_incremental(&mut hasher);
        // std.hash.autoHash(&hasher, self.flags);
        hasher.write_u8(self.flags.cval());
        hasher.finish()
    }

    pub fn generic(&self) -> GenericLeaf {
        GenericLeaf {
            flags: self.flags,
            actions: GenericLeafActions::Single([self.action.clone()]),
        }
    }
}

/// Leaf node of a set that triggers multiple actions in sequence.
#[derive(Clone, Debug, PartialEq)]
pub struct LeafChained {
    pub actions: Vec<Action>,
    pub flags: Flags,
}

impl LeafChained {
    pub fn clone_in(&self) -> LeafChained {
        // var cloned_actions = try self.actions.clone(alloc);
        // for (cloned_actions.items) |*action| action.* = try action.clone(alloc);
        let cloned_actions = self.actions.iter().map(|a| a.clone_action()).collect();
        LeafChained {
            actions: cloned_actions,
            flags: self.flags,
        }
    }

    pub fn generic(&self) -> GenericLeaf {
        GenericLeaf {
            flags: self.flags,
            actions: GenericLeafActions::Many(self.actions.clone()),
        }
    }
}

/// A generic leaf node that can be used to unify the handling of
/// leaf and leaf_chained.
///
/// PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD365. The reference's
/// `GenericLeaf.actions.many` is a slice INTO the set's `leaf_chained` list, so a
/// chained `reload_config`, which replaces the config (and the set) while the
/// chain runs, leaves the loop reading freed memory. The port's leaf OWNS a copy
/// of the actions, so it outlives the replaced set:
///
/// ```
/// use rustty_input::binding::{Action, Set, Trigger, TriggerKey, Value};
/// let mut s = Set::default();
/// s.parse_and_put(b"a=reload_config").unwrap();
/// s.parse_and_put(b"chain=text:world").unwrap();
/// let t = Trigger { key: TriggerKey::Unicode('a' as u32), ..Default::default() };
/// let leaf = match s.get_value(t).unwrap() {
///     Value::LeafChained(l) => l.generic(),
///     _ => unreachable!(),
/// };
/// // The config reload replaces the set mid-chain.
/// s = Set::default();
/// # let _ = &s;
/// assert_eq!(2, leaf.actions_slice().len());
/// assert!(leaf.actions_slice()[0] == Action::ReloadConfig);
/// ```
///
/// The reference's borrowed shape is rejected: the same program, with the leaf's
/// actions a slice into the set, does not compile (the set is replaced while
/// borrowed):
///
/// ```compile_fail,E0506
/// use rustty_input::binding::{Action, Set, Trigger, TriggerKey, Value};
/// let mut s = Set::default();
/// s.parse_and_put(b"a=reload_config").unwrap();
/// s.parse_and_put(b"chain=text:world").unwrap();
/// let t = Trigger { key: TriggerKey::Unicode('a' as u32), ..Default::default() };
/// let actions: &[Action] = match s.get_value(t).unwrap() {
///     Value::LeafChained(l) => &l.actions,
///     _ => unreachable!(),
/// };
/// // The config reload replaces the set mid-chain.
/// s = Set::default();
/// # let _ = &s;
/// assert_eq!(2, actions.len());
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GenericLeaf {
    pub flags: Flags,
    pub actions: GenericLeafActions,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GenericLeafActions {
    Single([Action; 1]),
    Many(Vec<Action>),
}

impl GenericLeaf {
    pub fn actions_slice(&self) -> &[Action] {
        match &self.actions {
            GenericLeafActions::Single(arr) => arr,
            GenericLeafActions::Many(slice) => slice,
        }
    }
}

/// The entry type for the forward mapping of trigger to action.
//
// PORT NOTE: `Set.Value = union(enum) { leader, leaf, leaf_chained }`. The
// `leader` arm owns its child `Set` (the reference stores `*Set` allocated with
// the arena; Rust owns it with `Box`).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// This key is a leader key in a sequence. You must follow the given
    /// set to find the next key in the sequence.
    Leader(Box<Set>),

    /// This trigger completes a sequence and the value is the action
    /// to take along with the flags that may define binding behavior.
    Leaf(Leaf),

    /// A set of actions to take in response to a trigger.
    LeafChained(LeafChained),
}

impl Value {
    /// Implements the formatter for the fmt package. This encodes the
    /// action back into the format used by parse.
    //
    // PORT NOTE: the reference `format` only handles `.leader`/`.leaf` (the
    // `.leaf_chained` arm is unhandled in `format`, which would be a comptime/
    // runtime error if reached — it is reachable only via `formatEntries`). We
    // preserve that: `format` panics for `leaf_chained` exactly as the reference
    // switch (which has no `leaf_chained` arm) would fail to compile/match.
    pub fn format<W: Writer>(&self, writer: &mut W) -> Result<(), W::Error> {
        match self {
            Value::Leader(set) => {
                // the leader key was already printed.
                for (k, _, v) in &set.bindings.entries {
                    // try writer.print(">{s}{s}", .{ binding.key_ptr.*, binding.value_ptr.* });
                    writer.write_all(b">")?;
                    k.format(writer)?;
                    v.format(writer)?;
                }
                Ok(())
            }
            Value::Leaf(leaf) => {
                // action implements the format
                writer.write_all(b"=")?;
                leaf.action.format(writer)
            }
            Value::LeafChained(_) => {
                // PORT NOTE: unreachable arm in the reference `format` (no
                // `leaf_chained` case); use `format_entries` for chained values.
                unreachable!("Value.format does not handle leaf_chained")
            }
        }
    }

    /// Writes the configuration entries for the binding
    /// that this value is part of.
    ///
    /// The value may be part of multiple configuration entries
    /// if they're all part of the same prefix sequence (e.g. 'a>b', 'a>c').
    /// These will result in multiple separate entries in the configuration.
    ///
    /// `buffer_stream` is a FixedBufferStream used for temporary storage
    /// that is shared between calls to nested levels of the set.
    /// For example, 'a>b>c=x' and 'a>b>d=y' will reuse the 'a>b' written
    /// to the buffer before flushing it to the formatter with 'c=x' and 'd=y'.
    //
    // PORT NOTE: the reference threads a `*std.Io.Writer` FIXED buffer (`buffer`)
    // that supports rewinding via `buffer.end = pos`. The Rust analog is a
    // `RewindBuffer` (a `Vec<u8>` with an explicit `end` cursor) so the `pos`/
    // `end` rewinding semantics — sharing a prefix between sibling entries — are
    // reproduced exactly. `formatter` is the cfg-fmt `EntryFormatter`.
    pub fn format_entries<W: Writer>(
        &self,
        buffer: &mut RewindBuffer,
        formatter: &mut EntryFormatter<W>,
    ) -> Result<(), FormatEntriesError<W::Error>> {
        match self {
            Value::Leader(set) => {
                // We'll rewind to this position after each sub-entry,
                // sharing the prefix between siblings.
                let pos = buffer.end;

                for (k, _, v) in &set.bindings.entries {
                    // I'm not exactly if this is safe for any arbitrary
                    // writer since the Writer interface does not have any
                    // rewind functions, but for our use case of a
                    // fixed-size buffer writer this should work just fine.
                    buffer.end = pos;
                    // buffer.print(">{f}", .{binding.key_ptr.*}) catch return error.OutOfMemory;
                    buffer
                        .write_all(b">")
                        .map_err(|_| FormatEntriesError::OutOfMemory)?;
                    write_trigger_to_buffer(k, buffer)
                        .map_err(|_| FormatEntriesError::OutOfMemory)?;
                    v.format_entries(buffer, formatter)?;
                }
                Ok(())
            }

            Value::Leaf(leaf) => {
                // When we get to the leaf, the buffer_stream contains
                // the full sequence of keys needed to reach this action.
                // buffer.print("={f}", .{leaf.action}) catch return error.OutOfMemory;
                buffer
                    .write_all(b"=")
                    .map_err(|_| FormatEntriesError::OutOfMemory)?;
                write_action_to_buffer(&leaf.action, buffer)
                    .map_err(|_| FormatEntriesError::OutOfMemory)?;
                // try formatter.formatEntry([]const u8, buffer.buffer[0..buffer.end]);
                formatter
                    .format_entry(&ByteSlice(buffer.slice().to_vec()))
                    .map_err(FormatEntriesError::from)
            }

            Value::LeafChained(leaf) => {
                let pos = buffer.end;
                for (i, action) in leaf.actions.iter().enumerate() {
                    if i == 0 {
                        // buffer.print("={f}", .{action})
                        buffer
                            .write_all(b"=")
                            .map_err(|_| FormatEntriesError::OutOfMemory)?;
                        write_action_to_buffer(action, buffer)
                            .map_err(|_| FormatEntriesError::OutOfMemory)?;
                    } else {
                        buffer.end = 0;
                        // buffer.print("chain={f}", .{action})
                        buffer
                            .write_all(b"chain=")
                            .map_err(|_| FormatEntriesError::OutOfMemory)?;
                        write_action_to_buffer(action, buffer)
                            .map_err(|_| FormatEntriesError::OutOfMemory)?;
                    }
                    formatter
                        .format_entry(&ByteSlice(buffer.slice().to_vec()))
                        .map_err(FormatEntriesError::from)?;
                    buffer.end = pos;
                }
                Ok(())
            }
        }
    }
}

/// PORT NOTE: error union of `formatEntries` — `error{OutOfMemory}` (from the
/// fixed buffer overflowing) plus the formatter writer's error.
pub enum FormatEntriesError<E> {
    OutOfMemory,
    Writer(E),
}

/// The `EntryFormatter::format_entry` error union (`FormatError`) folds into
/// `formatEntries`' inferred error set variant-for-variant — the reference's
/// `try formatter.formatEntry(...)` merges both `error.OutOfMemory` sources
/// into one inferred `!void` set.
impl<E> From<FormatError<E>> for FormatEntriesError<E> {
    fn from(e: FormatError<E>) -> FormatEntriesError<E> {
        match e {
            FormatError::Writer(w) => FormatEntriesError::Writer(w),
            FormatError::OutOfMemory => FormatEntriesError::OutOfMemory,
        }
    }
}

/// PORT NOTE: a `[]const u8` value wrapper implementing `FormatEntry` so it can
/// be passed to `EntryFormatter::format_entry` (the reference calls
/// `formatter.formatEntry([]const u8, bytes)` — the `.pointer` `[]const u8` arm,
/// `format_entry_string`).
struct ByteSlice(Vec<u8>);
impl FormatEntry for ByteSlice {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        format_entry_string(name, &self.0, writer)
    }
}

/// PORT NOTE: the reference's fixed `*std.Io.Writer` over a stack buffer, with an
/// `end` cursor that callers rewind (`buffer.end = pos`). `slice()` is
/// `buffer.buffer[0..buffer.end]`.
pub struct RewindBuffer {
    buf: Vec<u8>,
    end: usize,
}

/// `var buf: [1024]u8 = undefined;` — the capacity of the reference's stack
/// buffer (`Config.zig` `Keybinds.formatEntryDocs`, `RemapSet.formatEntry`).
/// The bound is OBSERVABLE: `std.Io.Writer.fixed(&buf)` fails once a write would
/// pass it, and every call site turns that into `error.OutOfMemory`.
pub const REWIND_BUFFER_CAPACITY: usize = 1024;

impl RewindBuffer {
    pub fn new() -> RewindBuffer {
        RewindBuffer {
            buf: Vec::new(),
            end: 0,
        }
    }
    // PORT NOTE: the `()` error models the reference's fixed-`std.Io.Writer`
    // overflow (`error.OutOfMemory`), which every caller maps to that error. The
    // buffer is heap-backed here (no 1 KiB of stack per format call), but it is
    // CAPPED at the reference's `[1024]u8` so the overflow boundary is identical:
    // a write that would push `end` past 1024 fails instead of growing.
    #[allow(clippy::result_unit_err)]
    pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), ()> {
        // A fixed `std.Io.Writer` fails the write once it no longer fits.
        if self.end + bytes.len() > REWIND_BUFFER_CAPACITY {
            return Err(());
        }
        // Mirrors writing into a fixed buffer at the `end` cursor, overwriting.
        if self.buf.len() < self.end {
            self.buf.resize(self.end, 0);
        }
        self.buf.truncate(self.end);
        self.buf.extend_from_slice(bytes);
        self.end += bytes.len();
        Ok(())
    }
    fn slice(&self) -> &[u8] {
        &self.buf[0..self.end]
    }
}

impl Default for RewindBuffer {
    fn default() -> Self {
        RewindBuffer::new()
    }
}

// Helpers to write a Trigger/Action through the RewindBuffer (the reference uses
// `{f}` format of the trigger/action). They reuse the `Writer` impl below.
//
// PORT NOTE: `pub` so the `Keybinds.formatEntryDocs` value type (rustty-config)
// can write the leading `{f}` trigger (and table-name prefix) into the shared
// `RewindBuffer` before delegating to `Value::format_entries`, exactly as the
// reference does (`writer.print("{f}", .{k})` / `"{s}/{f}"`).
// PORT NOTE: `()` error mirrors the fixed-buffer write error (produced when the
// write passes `REWIND_BUFFER_CAPACITY`); `result_unit_err` allowed for that
// fidelity.
#[allow(clippy::result_unit_err)]
pub fn write_trigger_to_buffer(t: &Trigger, buffer: &mut RewindBuffer) -> Result<(), ()> {
    let mut w = RewindWriter { inner: buffer };
    t.format(&mut w)
}
fn write_action_to_buffer(a: &Action, buffer: &mut RewindBuffer) -> Result<(), ()> {
    let mut w = RewindWriter { inner: buffer };
    a.format(&mut w)
}

pub(super) struct RewindWriter<'a> {
    pub(super) inner: &'a mut RewindBuffer,
}
impl Writer for RewindWriter<'_> {
    type Error = ();
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), ()> {
        self.inner.write_all(bytes)
    }
}

/// A structure that contains a set of bindings and focuses on fast lookup.
/// The use case is that this will be called on EVERY key input to look
/// for an associated action so it must be fast.
//
// PORT NOTE: the reference is allocator-managed (`HashMap`/`ReverseMap` are
// `ArrayHashMapUnmanaged` threaded with `alloc`), and uses raw `*Set`/`*Trigger`/
// `*Value` pointers for recursion and `chain_parent`. The Rust port owns its
// child sets via `Box<Set>` (in `Value::Leader`) and the maps via
// `CtxArrayHashMap`. `chain_parent` raw pointers (`*Trigger`/`*Value`/`*Set`) are
// modeled as a NAVIGABLE LOCATOR (`ChainParent`) — the path of leader triggers
// from the root to the containing set, plus the entry's key — so the entry can be
// re-resolved without aliasing UB (memory: raw-ptr discipline). The allocator
// (`alloc`) parameters are kept on the public methods as `()`-free signatures.
#[derive(Clone, Debug, PartialEq)]
pub struct Set {
    /// The set of bindings.
    bindings: CtxArrayHashMap<Trigger, Value, TriggerCtx>,

    /// The reverse mapping of action to binding. Note that multiple
    /// bindings can map to the same action and this map will only have
    /// the most recently added binding for an action.
    ///
    /// Sequenced triggers are never present in the reverse map at this time.
    /// This is a conscious decision since the primary use case of the reverse
    /// map is to support GUI toolkit keyboard accelerators and no mainstream
    /// GUI toolkit supports sequences.
    ///
    /// Performable triggers are also not present in the reverse map. This
    /// is so that GUI toolkits don't register performable triggers as
    /// menu shortcuts (the primary use case of the reverse map). GUI toolkits
    /// such as GTK handle menu shortcuts too early in the event lifecycle
    /// for performable to work so this is a conscious decision to ease the
    /// integration with GUI toolkits.
    reverse: CtxArrayHashMap<Action, Trigger, ActionCtx>,

    /// The chain parent is the information necessary to attach a chained
    /// action to the proper location in our mapping. It tracks both the
    /// entry in the hashmap and the set it belongs to, which is needed
    /// to properly update reverse mappings when converting a leaf to
    /// a chained action.
    chain_parent: Option<ChainParent>,
}

/// Information about a chain parent entry, including which set it
/// belongs to. This is needed because reverse mappings are only
/// maintained in the root set, but the chain parent entry may be
/// in a nested set (for leader key sequences).
//
// PORT NOTE: the reference fields are raw pointers `key_ptr: *Trigger`,
// `value_ptr: *Value`, `set: *Set`. Ported as a locator: `set_path` is the chain
// of leader triggers from the root set to the set containing the entry, and `key`
// is the entry's trigger. The entry is re-resolved on demand (see
// `chain_parent_*` helpers), preserving the observable behavior without holding
// aliasing raw pointers.
#[derive(Clone, Debug, PartialEq)]
pub struct ChainParent {
    pub set_path: Vec<Trigger>,
    pub key: Trigger,
}

impl Default for Set {
    fn default() -> Self {
        Set {
            bindings: CtxArrayHashMap::new(),
            reverse: CtxArrayHashMap::new(),
            chain_parent: None,
        }
    }
}

impl Set {
    // A full key-value entry for the set.
    //
    // PORT NOTE: `pub const Entry = HashMap.Entry;` — a (key, value) entry. The
    // Rust port represents an entry by its insertion index into the bindings map
    // (see `get_index`/`get_value`/`get_key`/`value_at`).

    // PORT NOTE: `deinit(self, alloc)` frees all child leader sets and the maps.
    // In Rust this is handled by `Drop`/ownership (`Box<Set>` children are freed
    // recursively), so no explicit `deinit` is needed; a no-op shim preserves the
    // call site shape (`s.deinit(alloc)`).
    pub fn deinit(&mut self) {
        // Clear any leaders if we have them
        // self.* = undefined; — handled by Rust ownership / Drop.
    }

    /// Get a binding for a given trigger.
    //
    // PORT NOTE: the reference returns `?Entry` (a key/value pointer pair). Here
    // we return the entry index; `get_value`/`get_key` read through it. Tests
    // that did `.?.value_ptr.*` use `get_value`.
    pub fn get_index(&self, t: Trigger) -> Option<usize> {
        self.bindings.get_index(&t)
    }

    /// `self.bindings.count()` — the number of bindings in this set.
    //
    // PORT NOTE: the reference reaches into `self.set.bindings.count()` directly
    // (the field is `pub`). `bindings` is private here, so this accessor mirrors
    // it for the `Keybinds` value type and tests.
    pub fn bindings_count(&self) -> usize {
        self.bindings.count()
    }

    /// `self.bindings.iterator()` — iterate `(trigger, value)` entries in
    /// insertion order.
    //
    // PORT NOTE: the reference iterates `self.set.bindings.iterator()` yielding
    // `entry.key_ptr.* / entry.value_ptr.*`. `bindings` is private here, so this
    // accessor exposes the same insertion-ordered view for `Keybinds.equalSet`
    // and `Keybinds.formatEntryDocs`.
    pub fn iter_bindings(&self) -> impl Iterator<Item = (&Trigger, &Value)> {
        self.bindings.entries.iter().map(|(k, _, v)| (k, v))
    }

    /// `get(t).?.value_ptr.*` — the value for a trigger.
    pub fn get_value(&self, t: Trigger) -> Option<&Value> {
        self.bindings.get(&t)
    }

    /// `get(t).?.key_ptr.*` — the stored key for a trigger.
    pub fn get_key(&self, t: Trigger) -> Option<&Trigger> {
        self.bindings.get_index(&t).map(|i| self.bindings.key_at(i))
    }

    /// Get a trigger for the given action. An action can have multiple
    /// triggers so this will return the first one found.
    pub fn get_trigger(&self, a: &Action) -> Option<Trigger> {
        self.reverse.get(a).copied()
    }

    /// Whether the set currently has a chain parent (for tests).
    pub fn chain_parent(&self) -> Option<&ChainParent> {
        self.chain_parent.as_ref()
    }

    /// Resolve the `Set` containing the chain-parent entry (follows `set_path`).
    fn resolve_set_mut(&mut self, path: &[Trigger]) -> &mut Set {
        let mut cur: &mut Set = self;
        for t in path {
            let idx = cur.bindings.get_index(t).expect("chain path leader exists");
            match cur.bindings.value_at_mut(idx) {
                Value::Leader(s) => cur = s.as_mut(),
                _ => unreachable!("chain path entry must be a leader"),
            }
        }
        cur
    }
}

impl Set {
    /// Add a binding to the set. If the binding already exists then
    /// this will overwrite it.
    pub fn put(&mut self, t: Trigger, action: Action) {
        self.put_flags(t, action, Flags::default());
    }

    /// Add a binding to the set with explicit flags.
    //
    // PORT NOTE: the reference uses `getOrPut` returning raw `key_ptr`/`value_ptr`
    // which it stores in `chain_parent`. We reproduce the same effect: do the
    // get-or-put, set the value to a leaf, update the reverse map, and record the
    // chain_parent as `{ set_path: [], key: t }` (this method always operates on
    // `self`, i.e. the set on which it is called).
    pub fn put_flags(&mut self, t: Trigger, action: Action, flags: Flags) {
        // unbind should never go into the set, it should be handled prior
        debug_assert!(action != Action::Unbind);

        // This is true if we're going to track this entry as
        // a reverse mapping. There are certain scenarios we don't.
        // See the reverse map docs for more information.
        let track_reverse: bool = !flags.performable;

        // No matter what our chained parent becomes invalid because
        // getOrPut invalidates pointers.
        self.chain_parent = None;

        let (idx, found_existing) = self.bindings.get_or_put_with(t, || {
            Value::Leaf(Leaf {
                action: action.clone(),
                flags,
            })
        });
        // errdefer {
        //     // If we have any errors we can't trust our values here. And
        //     // we can't restore the old values because they're also invalidated
        //     // by getOrPut so we just disable chaining.
        //     self.chain_parent = null;
        // }
        //
        // PORT NOTE: the `getOrPut` here is infallible in the Rust port (no
        // allocator error path), so the reference's `errdefer` clearing
        // `chain_parent` cannot fire; its comment is preserved above.

        if found_existing {
            // PORT NOTE: the reference reads the existing value via `gop.value_ptr`
            // and cleans it up before overwriting.
            match self.bindings.value_at(idx) {
                // If we have a leader we need to clean up the memory
                Value::Leader(_) => {
                    // Box<Set> child is dropped when we overwrite below.
                }

                // If we have an existing binding for this trigger, we have to
                // update the reverse mapping to remove the old action.
                Value::Leaf(_) => {
                    if track_reverse {
                        let t_hash = t.hash();
                        let n = self.reverse.count();
                        for i in 0..n {
                            if t_hash == self.reverse.value_at(i).hash() {
                                self.reverse.swap_remove_at(i);
                                break;
                            }
                        }
                    }
                }

                // Chained leaves aren't in the reverse mapping so we just
                // clear it out.
                Value::LeafChained(_) => {
                    // dropped on overwrite below.
                }
            }
        }

        // gop.value_ptr.* = .{ .leaf = .{ .action = action, .flags = flags } };
        *self.bindings.value_at_mut(idx) = Value::Leaf(Leaf {
            action: action.clone(),
            flags,
        });

        // if (track_reverse) try self.reverse.put(alloc, action, t);
        if track_reverse {
            self.reverse.put(action, t);
        }

        // Record chain parent (this set, the entry we just added/updated).
        self.chain_parent = Some(ChainParent {
            set_path: Vec::new(),
            key: t,
        });

        // Invariant: after successful put, chain_parent must be valid and point
        // to the entry we just added/updated.
        debug_assert!(self.chain_parent.is_some());
        debug_assert!(matches!(self.bindings.value_at(idx), Value::Leaf(_)));
    }

    /// Get an entry for the given key event. This will attempt to find
    /// a binding using multiple parts of the event in the following order:
    ///
    ///   1. Physical key (event.physical_key)
    ///   2. Unshifted Unicode codepoint (event.unshifted_codepoint)
    ///
    pub fn get_event(&self, event: &KeyEvent) -> Option<usize> {
        let mut trigger: Trigger = Trigger {
            mods: event.mods.binding(),
            key: TriggerKey::Physical(event.key),
        };
        if let Some(v) = self.get_index(trigger) {
            return Some(v);
        }

        // If our UTF-8 text is exactly one codepoint, we try to match that.
        if !event.utf8.is_empty() {
            'unicode: {
                let s = match std::str::from_utf8(&event.utf8) {
                    Ok(s) => s,
                    Err(_) => break 'unicode,
                };
                let mut it = s.chars();
                // No codepoints or multiple codepoints drops to invalid format
                let cp = match it.next() {
                    Some(c) => c,
                    None => break 'unicode,
                };
                if it.next().is_some() {
                    break 'unicode;
                }

                trigger.key = TriggerKey::Unicode(cp as u21);
                if let Some(v) = self.get_index(trigger) {
                    return Some(v);
                }
            }
        }

        // Finally fallback to the full unshifted codepoint if we have one.
        // Question: should we be doing this if we have UTF-8 text? I
        // suspect "no" but we don't currently have any failing scenarios
        // to verify this.
        if event.unshifted_codepoint > 0 {
            trigger.key = TriggerKey::Unicode(event.unshifted_codepoint);
            if let Some(v) = self.get_index(trigger) {
                return Some(v);
            }
        }

        // Fallback to catch_all with modifiers first, then without modifiers.
        trigger.key = TriggerKey::CatchAll;
        if let Some(v) = self.get_index(trigger) {
            return Some(v);
        }
        if !trigger.mods.empty() {
            trigger.mods = Mods::default();
            if let Some(v) = self.get_index(trigger) {
                return Some(v);
            }
        }

        None
    }

    /// Get the leaf value at an entry index (test helper for `getEvent`).
    pub fn value_at(&self, index: usize) -> &Value {
        self.bindings.value_at(index)
    }

    /// Get the stored trigger key at an entry index.
    //
    // PORT NOTE (additive, core C10 — CORE_BLUEPRINT risk 7): the reference
    // `Set.Entry` is a key/value POINTER pair; the ported `getEvent` returns
    // the entry INDEX (see `get_index`), so `entry.key_ptr.*` reads
    // (Surface.maybeHandleBinding's `.key_sequence` trigger payload) go
    // through this accessor — the `value_at` sibling.
    pub fn key_at(&self, index: usize) -> &Trigger {
        self.bindings.key_at(index)
    }
}

impl Set {
    /// Remove a binding for a given trigger.
    pub fn remove(&mut self, t: Trigger) {
        self.remove_exact(t);
    }

    fn remove_exact(&mut self, t: Trigger) {
        // Removal always resets our chain parent. We could make this
        // finer grained but the way it is documented is that chaining
        // must happen directly after sets so this works.
        self.chain_parent = None;

        // var entry = self.bindings.get(t) orelse return;
        // _ = self.bindings.swapRemove(t);
        let entry = match self.bindings.swap_remove(&t) {
            Some(e) => e,
            None => return,
        };

        match entry {
            // For a leader removal, we need to deallocate our child set.
            // Leaders are never part of reverse maps so no other accounting
            // needs to be done.
            Value::Leader(_) => {
                // Box<Set> child dropped here.
            }

            // For an action we need to fix up the reverse mapping.
            Value::Leaf(leaf) => self.fixup_reverse_for_action(&leaf.action, t),

            // Chained leaves are never in our reverse mapping so no
            // cleanup is required.
            Value::LeafChained(_) => {
                // dropped here.
            }
        }
    }

    /// Fix up the reverse mapping after removing an action.
    ///
    /// When an action is removed from a binding (either by removal or by
    /// converting to a chained action), we need to update the reverse mapping.
    /// If another binding has the same action, we update the reverse mapping
    /// to point to that binding. Otherwise, we remove the action from the
    /// reverse mapping entirely.
    ///
    /// The `old` parameter is the trigger that was previously bound to this
    /// action. It is used to check if the reverse mapping still points to
    /// this trigger; if not, no fixup is needed since the reverse map already
    /// points to a different trigger for this action.
    ///
    /// Note: we'd LIKE to replace this with the most recent binding but
    /// our hash map obviously has no concept of ordering so we have to
    /// choose whatever. Maybe a switch to an array hash map here.
    fn fixup_reverse_for_action(&mut self, action: &Action, old: Trigger) {
        // const entry = self.reverse.getEntry(action) orelse return;
        let entry_idx = match self.reverse.get_index(action) {
            Some(i) => i,
            None => return,
        };

        // If our value is not the same as the old trigger, we can
        // ignore it because our reverse mapping points somewhere else.
        if !self.reverse.value_at(entry_idx).equal(old) {
            return;
        }

        // It is the same trigger, so let's now go through our bindings
        // and try to find another trigger that maps to the same action.
        let action_hash = action.hash();
        let n = self.bindings.count();
        for i in 0..n {
            match self.bindings.value_at(i) {
                Value::Leader(_) | Value::LeafChained(_) => {}
                Value::Leaf(leaf_search) => {
                    if leaf_search.action.hash() == action_hash {
                        let key = *self.bindings.key_at(i);
                        *self.reverse.value_at_mut(entry_idx) = key;
                        return;
                    }
                }
            }
        }

        // No other trigger points to this action so we remove
        // the reverse mapping completely.
        let _ = self.reverse.swap_remove(action);
    }

    /// Append a chained action to the prior set action.
    ///
    /// It is an error if there is no valid prior chain parent.
    pub fn append_chain(&mut self, action: Action) -> Result<(), AppendChainError> {
        // Unbind is not a valid chain action; callers must check this.
        debug_assert!(action != Action::Unbind);

        // const parent = self.chain_parent orelse return error.NoChainParent;
        let parent = match self.chain_parent.clone() {
            Some(p) => p,
            None => return Err(AppendChainError::NoChainParent),
        };

        // Resolve the parent set + entry.
        let path = parent.set_path.clone();
        let pkey = parent.key;

        // We need the parent value to decide leaf vs leaf_chained.
        let parent_set = self.resolve_set_mut(&path);
        let pidx = parent_set
            .bindings
            .get_index(&pkey)
            .expect("chain_parent entry exists");

        match parent_set.bindings.value_at(pidx) {
            // Leader can never be a chain parent. Verified through various
            // assertions and unit tests.
            Value::Leader(_) => unreachable!(),

            // If it is already a chained action, we just append the
            // action. Easy!
            Value::LeafChained(_) => {
                if let Value::LeafChained(leaf) = parent_set.bindings.value_at_mut(pidx) {
                    leaf.actions.push(action);
                }
                Ok(())
            }

            // If it is a leaf, we need to convert it to a leaf_chained.
            // We also need to be careful to remove any prior reverse
            // mappings for this action since chained actions are not
            // part of the reverse mapping.
            Value::Leaf(leaf) => {
                let leaf_action = leaf.action.clone();
                let leaf_flags = leaf.flags;

                // Setup our failable actions list first.
                // var actions: std.ArrayList(Action) = .empty;
                // try actions.ensureTotalCapacity(alloc, 2);
                // actions.appendAssumeCapacity(leaf.action);
                // actions.appendAssumeCapacity(action);
                //
                // PORT NOTE: mirrors the reference's reserve-then-append-assume-
                // capacity sequence (kept literal rather than `vec![..]`).
                #[allow(clippy::vec_init_then_push)]
                let actions: Vec<Action> = {
                    let mut actions: Vec<Action> = Vec::with_capacity(2);
                    actions.push(leaf_action.clone());
                    actions.push(action);
                    actions
                };

                // Convert to leaf_chained first, before fixing up reverse
                // mapping. This is important because fixupReverseForAction
                // searches for other bindings with the same action, and we
                // don't want to find this entry (which is now chained).
                *parent_set.bindings.value_at_mut(pidx) = Value::LeafChained(LeafChained {
                    actions,
                    flags: leaf_flags,
                });

                // Clean up our reverse mapping. Chained actions are not
                // part of the reverse mapping, so we need to fix up the
                // reverse map (possibly restoring another trigger for the
                // same action).
                //
                // PORT NOTE: `parent.set.fixupReverseForAction(...)` operates on
                // the set that OWNS the entry. We re-resolve it and call fixup.
                let parent_set = self.resolve_set_mut(&path);
                parent_set.fixup_reverse_for_action(&leaf_action, pkey);
                Ok(())
            }
        }
    }

    /// Deep clone the set.
    pub fn clone_set(&self) -> Set {
        // var result: Set = .{ .bindings = try self.bindings.clone(alloc),
        //                      .reverse = try self.reverse.clone(alloc) };
        let mut result = Set {
            bindings: CtxArrayHashMap::new(),
            reverse: CtxArrayHashMap::new(),
            // Clone should have null chain_parent.
            chain_parent: None,
        };

        // If we have any leaders we need to clone them.
        for (k, h, v) in &self.bindings.entries {
            let cloned = match v {
                // Leaves could have data to clone (i.e. text actions
                // contain allocated strings).
                Value::Leaf(s) => Value::Leaf(s.clone_in()),
                Value::LeafChained(s) => Value::LeafChained(s.clone_in()),
                // Must be deep cloned.
                Value::Leader(s) => Value::Leader(Box::new(s.clone_set())),
            };
            result.bindings.entries.push((*k, *h, cloned));
        }

        // We need to clone the action keys in the reverse map since
        // they may contain allocated values.
        for (a, h, t) in &self.reverse.entries {
            result.reverse.entries.push((a.clone_action(), *h, *t));
        }

        result
    }
}

/// PORT NOTE: error union of `appendChain` — `Allocator.Error || error{NoChainParent}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppendChainError {
    NoChainParent,
}

/// PORT NOTE: error union of `parseAndPut` — `Allocator.Error || Error`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseAndPutError {
    InvalidFormat,
    InvalidAction,
}

impl From<Error> for ParseAndPutError {
    fn from(e: Error) -> ParseAndPutError {
        match e {
            Error::InvalidFormat => ParseAndPutError::InvalidFormat,
            Error::InvalidAction => ParseAndPutError::InvalidAction,
        }
    }
}

// const ParseAndPutRecurseError = Allocator.Error || error{
//     SequenceUnbind, NoChainParent, UnexpectedEndOfInput, InvalidChainAction };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecurseError {
    SequenceUnbind,
    NoChainParent,
    UnexpectedEndOfInput,
    InvalidChainAction,
}

impl Set {
    /// Parse a user input binding and add it to the set. This will handle
    /// the "unbind" case, ensure consumed/unconsumed fields are set correctly,
    /// handle sequences, etc.
    ///
    /// If this returns an OutOfMemory error then the set is in a broken
    /// state and should not be used again. Any Error returned is validated
    /// before any set modifications are made.
    pub fn parse_and_put(&mut self, input: &[u8]) -> Result<(), ParseAndPutError> {
        // To make cleanup easier, we ensure that the full sequence is
        // valid before making any set modifications. This is more expensive
        // computationally but it makes cleanup way, way easier.
        let mut it = Parser::init(input)?;
        while it.next()?.is_some() {}
        it.reset();

        // We use recursion so that we can utilize the stack as our state
        // for cleanup.
        let mut path: Vec<Trigger> = Vec::new();
        let updated_set_ = match self.parse_and_put_recurse(&mut path, &mut it) {
            Ok(v) => v,
            Err(err) => {
                match err {
                    // If this gets sent up to the root then we've unbound
                    // all the way up and this put was a success.
                    RecurseError::SequenceUnbind => None,

                    // If our parser input was too short then the format
                    // is invalid because we handle all valid cases.
                    RecurseError::UnexpectedEndOfInput => {
                        return Err(ParseAndPutError::InvalidFormat)
                    }

                    // If we had a chain without a parent then the format is wrong.
                    RecurseError::NoChainParent => return Err(ParseAndPutError::InvalidFormat),

                    // If we had an invalid action for a chain (e.g. unbind).
                    RecurseError::InvalidChainAction => {
                        return Err(ParseAndPutError::InvalidFormat)
                    }
                }

                // Errors must never fall through.
                // unreachable;
                //
                // PORT NOTE: the reference's `OutOfMemory` arm (`return
                // error.OutOfMemory`) has no Rust counterpart — the recursion is
                // infallible w.r.t. allocation — so the match above is exhaustive
                // and there is no fall-through `unreachable` to reach.
            }
        };

        // If we have an updated set (a binding was added) then we store
        // it for our chain parent. If we didn't update a set then we clear
        // our chain parent since chaining is no longer valid until a
        // valid binding is saved.
        match updated_set_ {
            Some(updated_path) => {
                // A successful addition must have recorded a chain parent on the
                // updated set; rebuild the root chain_parent with the FULL path.
                let updated_set = self.resolve_set_mut(&updated_path);
                let cp = updated_set
                    .chain_parent
                    .clone()
                    .expect("updated set recorded a chain parent");
                // The updated set's chain_parent path is relative to that set;
                // prepend the path to the updated set to make it root-relative.
                let mut full_path = updated_path.clone();
                full_path.extend(cp.set_path.iter().copied());
                self.chain_parent = Some(ChainParent {
                    set_path: full_path,
                    key: cp.key,
                });
                debug_assert!(self.chain_parent.is_some());
            }
            None => {
                self.chain_parent = None;
            }
        }

        Ok(())
    }

    /// Returns the set that was ultimately updated if a binding was
    /// added. Unbind does not return a set since nothing was added.
    //
    // PORT NOTE: the reference threads `root: *Set` and `set: *Set`. Here the
    // method runs on `self` (the root) and `path` identifies the CURRENT set
    // (chain of leader triggers from root). The returned `Option<Vec<Trigger>>` is
    // the path to the updated set (the reference's returned `?*Set`).
    fn parse_and_put_recurse(
        &mut self,
        path: &mut Vec<Trigger>,
        it: &mut Parser,
    ) -> Result<Option<Vec<Trigger>>, RecurseError> {
        let elem = match it.next() {
            // `it.next() catch unreachable`
            Ok(v) => v,
            Err(_) => unreachable!(),
        }
        .ok_or(RecurseError::UnexpectedEndOfInput)?;

        match elem {
            Elem::Leader(t) => {
                // If we have a leader, we need to upsert a set for it.
                // Since we remove the value, we need to copy it.
                let cur = self.resolve_set_mut(path);
                let old: Option<Value> = cur.bindings.get(&t).cloned();
                if let Some(entry) = &old {
                    match entry {
                        // We have an existing leader for this key already
                        // so recurse into this set.
                        Value::Leader(_) => {
                            path.push(t);
                            let r = self.parse_and_put_recurse(path, it);
                            path.pop();
                            return match r {
                                // Our child put unbound. If our set is empty we
                                // need to dealloc and continue up. If our set is
                                // not empty then we're done.
                                Err(RecurseError::SequenceUnbind) => {
                                    let child_empty = {
                                        let cur = self.resolve_set_mut(path);
                                        let idx = cur.bindings.get_index(&t).unwrap();
                                        match cur.bindings.value_at(idx) {
                                            Value::Leader(s) => s.bindings.count() == 0,
                                            _ => unreachable!(),
                                        }
                                    };
                                    if child_empty {
                                        let cur = self.resolve_set_mut(path);
                                        cur.remove(t);
                                        Err(RecurseError::SequenceUnbind)
                                    } else {
                                        Ok(None)
                                    }
                                }
                                Err(e) => Err(e),
                                Ok(v) => Ok(v),
                            };
                        }

                        Value::Leaf(_) | Value::LeafChained(_) => {
                            // Remove the existing action. Fallthrough as if
                            // we don't have a leader.
                            let cur = self.resolve_set_mut(path);
                            cur.remove(t);
                        }
                    }
                }

                // Create our new set for this leader
                // Insert the leader entry
                {
                    let cur = self.resolve_set_mut(path);
                    cur.bindings.put(t, Value::Leader(Box::<Set>::default()));
                }

                // Recurse
                path.push(t);
                let r = self.parse_and_put_recurse(path, it);
                path.pop();
                match r {
                    // If our action was to unbind, we restore the old
                    // action if we have it.
                    Err(RecurseError::SequenceUnbind) => {
                        let cur = self.resolve_set_mut(path);
                        cur.remove(t);
                        if let Some(entry) = old {
                            match entry {
                                Value::Leader(_) => unreachable!(), // Handled above
                                Value::Leaf(leaf) => {
                                    let cur = self.resolve_set_mut(path);
                                    // set.putFlags(...) catch {}
                                    cur.put_flags(t, leaf.action, leaf.flags);
                                }
                                Value::LeafChained(leaf) => 'chain: {
                                    // Rebuild our chain
                                    {
                                        let cur = self.resolve_set_mut(path);
                                        cur.put_flags(t, leaf.actions[0].clone(), leaf.flags);
                                    }
                                    for action in &leaf.actions[1..] {
                                        let cur = self.resolve_set_mut(path);
                                        match cur.append_chain(action.clone()) {
                                            Ok(()) => {}
                                            Err(_) => {
                                                let cur = self.resolve_set_mut(path);
                                                cur.remove(t);
                                                break 'chain;
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        Ok(None)
                    }

                    Err(e) => Err(e),
                    Ok(v) => Ok(v),
                }
            }

            Elem::Binding(b) => match b.action {
                Action::Unbind => {
                    let cur = self.resolve_set_mut(path);
                    cur.remove(b.trigger);
                    Err(RecurseError::SequenceUnbind)
                }

                _ => {
                    let cur = self.resolve_set_mut(path);
                    cur.put_flags(b.trigger, b.action, b.flags);
                    Ok(Some(path.clone()))
                }
            },

            Elem::Chain(action) => {
                // Chains can only happen on the root.
                debug_assert!(path.is_empty());
                // Unbind is not valid for chains.
                if action == Action::Unbind {
                    return Err(RecurseError::InvalidChainAction);
                }
                let cur = self.resolve_set_mut(path);
                match cur.append_chain(action) {
                    Ok(()) => Ok(Some(path.clone())),
                    Err(AppendChainError::NoChainParent) => Err(RecurseError::NoChainParent),
                }
            }
        }
    }
}
