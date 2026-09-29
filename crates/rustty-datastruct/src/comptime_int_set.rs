//! A fixed set of integers that is known at compile time and can be
//! searched quickly at runtime.
//!
//! PORTS: ghostty src/datastruct/comptime_int_set.zig
//!
//! PORT STATUS: done
//
// PORT NOTE: the reference is `fn ComptimeIntSet(comptime T: type, comptime
// keys: []const T) type`, a type generated at comptime whose sorted table is a
// comptime constant. Rust has no comptime type generation and no const trait
// comparisons, so the port is a value type `ComptimeIntSet<T, N>` holding the
// sorted `[(key, original_index); N]` table, built by a `const fn new` that a
// caller evaluates in a `const` item (so the sort and the duplicate check run
// at compile time and a duplicate key is a compile error, exactly as the
// reference's `@compileError`). `new` is generated per primitive integer type
// by `impl_comptime_int_set!` because stable `const fn` cannot compare a
// generic `T`. The lookup is `#[inline]` and its loop runs over the const
// generic `N`, so it unrolls into a fixed number of steps like the reference's
// `inline while` (never a runtime `HashMap`/`OnceLock`; see the
// comptime-flattening perf note).
//
// The reference's `Index` is `std.math.IntFittingRange(0, len - 1)`, the
// smallest unsigned integer that holds every index. The port stores each
// original index as a `u16` (`new` asserts `N <= 65536`) and `index_of`
// returns it widened to `usize`, so the result indexes any table directly.

/// A fixed set of integers that is known at compile time and can be
/// searched quickly at runtime.
///
/// Use this when you have a table of integer IDs defined at comptime and
/// need to find which entry a runtime value belongs to. The common way to
/// write that is a loop over the table:
///
/// ```text
///     inline for (entries, 0..) |entry, i| {
///         if (entry.id == id) return i;
///     }
///     return null;
/// ```
///
/// That loop compiles to one comparison per entry. Entries near the end
/// of the table, and values that aren't in the table at all, pay for every
/// comparison before them. This type sorts the keys at comptime and does
/// a binary search instead, so every lookup costs about log2(N)
/// comparisons no matter which key is looked up. The search has a fixed
/// number of steps and no loop, which lets the compiler emit it without
/// branches.
///
/// `index_of` returns the position of the key in the `keys` slice you
/// passed in, not its position in the sorted order. This means you can
/// use the result to index any other table that is in the same order as
/// `keys`:
///
/// ```
/// use rustty_datastruct::ComptimeIntSet;
///
/// struct Entry {
///     id: u16,
///     name: &'static str,
/// }
/// const ENTRIES: [Entry; 3] = [
///     Entry { id: 2026, name: "synchronized_output" },
///     Entry { id: 25, name: "cursor_visible" },
///     Entry { id: 1049, name: "alt_screen" },
/// ];
///
/// const IDS: ComptimeIntSet<u16, 3> = ComptimeIntSet::<u16, 3>::new([2026, 25, 1049]);
///
/// fn name_of(id: u16) -> Option<&'static str> {
///     let i = IDS.index_of(id)?;
///     Some(ENTRIES[i].name)
/// }
///
/// assert_eq!(name_of(25), Some("cursor_visible"));
/// assert_eq!(name_of(26), None);
/// # let _ = ENTRIES[0].id;
/// ```
///
/// `T` must be an integer type. Keys can be in any order but must be
/// unique. Duplicate keys are a compile error:
///
/// ```compile_fail
/// use rustty_datastruct::ComptimeIntSet;
/// const DUP: ComptimeIntSet<u16, 3> = ComptimeIntSet::<u16, 3>::new([1, 2, 1]);
/// let _ = DUP.contains(1);
/// ```
///
/// For a handful of keys a plain loop or `switch` is just as fast and
/// simpler. This is worth reaching for when the table has dozens of
/// entries and the lookup is on a hot path.
// PORT NOTE: `Copy` so a `const` set can be used by value in any context.
#[derive(Clone, Copy, Debug)]
pub struct ComptimeIntSet<T, const N: usize> {
    /// The keys in ascending order, each paired with its index in the
    /// original `keys` slice. That is, `table[i].0 == keys[table[i].1]`.
    // PORT NOTE: the reference's `Table { sorted: [len]T, original:
    // [len]Index }` pair of arrays, zipped into one array of pairs.
    table: [(T, u16); N],
}

impl<T, const N: usize> ComptimeIntSet<T, N> {
    /// The number of keys in the set.
    pub const LEN: usize = N;
}

macro_rules! impl_comptime_int_set {
    ($($t:ty),* $(,)?) => {$(
        impl<const N: usize> ComptimeIntSet<$t, N> {
            /// Build the set from `keys`. Evaluate it in a `const` item so the
            /// sort and the uniqueness check run at compile time; a duplicate
            /// key then fails the build.
            pub const fn new(keys: [$t; N]) -> Self {
                // PORT NOTE: the original index is stored as a u16 (see the
                // module PORT NOTE on `Index`).
                assert!(N <= u16::MAX as usize + 1, "ComptimeIntSet has too many keys");

                let mut table: [($t, u16); N] = [(0, 0); N];
                let mut i = 0;
                while i < N {
                    table[i] = (keys[i], i as u16);
                    i += 1;
                }

                // PORT NOTE: the reference sorts the index order with
                // `std.mem.sortUnstable`; `const fn` has no sort, so this is
                // an insertion sort by key. Keys are unique, so stability is
                // unobservable.
                let mut i = 1;
                while i < N {
                    let mut j = i;
                    while j > 0 && table[j - 1].0 > table[j].0 {
                        let tmp = table[j - 1];
                        table[j - 1] = table[j];
                        table[j] = tmp;
                        j -= 1;
                    }
                    i += 1;
                }

                let mut i = 1;
                while i < N {
                    if table[i - 1].0 == table[i].0 {
                        panic!("ComptimeIntSet keys must be unique");
                    }
                    i += 1;
                }

                ComptimeIntSet { table }
            }

            /// Returns the index of `key` in the original `keys` slice, or
            /// null if `key` is not in the set.
            #[inline]
            pub fn index_of(&self, key: $t) -> Option<usize> {
                if N == 0 {
                    return None;
                }

                // Binary search for the last sorted key that is <= `key`.
                // Each step halves the range that is left. `n` is comptime
                // known, so this unrolls into a fixed number of steps.
                let mut base: usize = 0;
                let mut n: usize = N;
                while n > 1 {
                    let half = n / 2;
                    base = if self.table[base + half].0 <= key {
                        base + half
                    } else {
                        base
                    };
                    n -= half;
                }

                if self.table[base].0 != key {
                    return None;
                }
                Some(self.table[base].1 as usize)
            }

            /// Returns true if `key` is in the set.
            #[inline]
            pub fn contains(&self, key: $t) -> bool {
                self.index_of(key).is_some()
            }
        }
    )*};
}

impl_comptime_int_set!(u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize);

#[cfg(test)]
mod tests {
    use super::*;

    // test "ComptimeIntSet: empty"
    #[test]
    fn comptime_int_set_empty() {
        const SET: ComptimeIntSet<u16, 0> = ComptimeIntSet::<u16, 0>::new([]);
        assert!(SET.index_of(0).is_none());
        assert!(!SET.contains(1));
    }

    // test "ComptimeIntSet: single"
    #[test]
    fn comptime_int_set_single() {
        const SET: ComptimeIntSet<u16, 1> = ComptimeIntSet::<u16, 1>::new([42]);
        assert_eq!(0, SET.index_of(42).unwrap());
        assert!(SET.index_of(41).is_none());
        assert!(SET.index_of(43).is_none());
    }

    // test "ComptimeIntSet: unsorted keys return original index"
    #[test]
    fn comptime_int_set_unsorted_keys_return_original_index() {
        const KEYS: [u16; 7] = [2026, 4, 1049, 25, 1, 65535, 0];
        const SET: ComptimeIntSet<u16, 7> = ComptimeIntSet::<u16, 7>::new(KEYS);
        for (i, &key) in KEYS.iter().enumerate() {
            assert_eq!(i, SET.index_of(key).unwrap());
        }
    }

    // test "ComptimeIntSet: exhaustive against linear scan"
    //
    // PORT NOTE: the reference's `inline for (0..20) |n|` builds a comptime
    // key array and a set per length. The port monomorphizes `check::<N>` per
    // length; `Keys<N>::SET` is an associated const, so each set is still
    // built at compile time.
    #[test]
    fn comptime_int_set_exhaustive_against_linear_scan() {
        struct Keys<const N: usize>;
        impl<const N: usize> Keys<N> {
            const KEYS: [u8; N] = {
                let mut result = [0u8; N];
                let mut i = 0;
                while i < N {
                    result[i] = ((i * 37 + 11) % 251) as u8;
                    i += 1;
                }
                result
            };
            const SET: ComptimeIntSet<u8, N> = ComptimeIntSet::<u8, N>::new(Self::KEYS);
        }

        fn check<const N: usize>() {
            let keys = Keys::<N>::KEYS;
            for v in 0..256usize {
                let expected: Option<usize> = keys.iter().position(|&key| key as usize == v);
                let actual: Option<usize> = Keys::<N>::SET.index_of(v as u8);
                assert_eq!(expected, actual, "n={N} v={v}");
            }
        }

        // The search unrolls differently for each length, so test every
        // length from 0 up past a few powers of two.
        check::<0>();
        check::<1>();
        check::<2>();
        check::<3>();
        check::<4>();
        check::<5>();
        check::<6>();
        check::<7>();
        check::<8>();
        check::<9>();
        check::<10>();
        check::<11>();
        check::<12>();
        check::<13>();
        check::<14>();
        check::<15>();
        check::<16>();
        check::<17>();
        check::<18>();
        check::<19>();
    }

    // test "ComptimeIntSet: signed"
    #[test]
    fn comptime_int_set_signed() {
        const SET: ComptimeIntSet<i8, 5> = ComptimeIntSet::<i8, 5>::new([5, -3, 0, -128, 127]);
        assert_eq!(1, SET.index_of(-3).unwrap());
        assert_eq!(3, SET.index_of(-128).unwrap());
        assert_eq!(4, SET.index_of(127).unwrap());
        assert!(SET.index_of(-4).is_none());
    }
}
