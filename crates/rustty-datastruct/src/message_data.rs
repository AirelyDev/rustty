//! Creates a union that can be used to accommodate data that fit within an array,
//! are a stable pointer, or require deallocation. This is helpful for thread
//! messaging utilities.
//
// PORT NOTE: The Zig reference is a `fn MessageData(Elem, small_size) type`
// returning a tagged union. We model it with a generic enum `MessageData<Elem,
// const SMALL_SIZE>`. The `Stable` variant in Zig is a borrowed `[]const Elem`;
// in Rust we represent the owned/stable/small distinction with an enum whose
// `Stable` holds a `&'static [Elem]`-style borrow modeled via a boxed slice for
// owned-stable inputs in tests is not needed — the tests only construct via
// `init`, which yields `small` or `alloc`. The `len` type
// `IntFittingRange(0, small_size)` is reproduced as the smallest unsigned int
// that can hold `small_size`; we store it as `usize` since the byte layout is
// not compared anywhere (no test inspects raw bytes), preserving all logic.

/// A small write where the data fits into this union size.
pub struct Small<Elem: Copy + Default, const MAX: usize> {
    pub data: [Elem; MAX],
    pub len: usize,
}

/// Allocated and must be freed with the provided allocator. This
/// should be rarely used.
///
/// In the Zig reference this stores the `Allocator` plus the slice; in Rust
/// the owned heap buffer (`Box<[Elem]>`) carries its allocation, so `deinit`
/// is just `Drop`.
pub struct Alloc<Elem> {
    pub data: Box<[Elem]>,
}

/// Creates a union that can be used to accommodate data that fit within an array,
/// are a stable pointer, or require deallocation. This is helpful for thread
/// messaging utilities.
pub enum MessageData<'a, Elem: Copy + Default, const SMALL_SIZE: usize> {
    /// A small write where the data fits into this union size.
    Small(Small<Elem, SMALL_SIZE>),

    /// A stable pointer so we can just pass the slice directly through.
    /// This is useful i.e. for const data.
    Stable(&'a [Elem]),

    /// Allocated and must be freed with the provided allocator. This
    /// should be rarely used.
    Alloc(Alloc<Elem>),
}

impl<'a, Elem: Copy + Default, const SMALL_SIZE: usize> MessageData<'a, Elem, SMALL_SIZE> {
    /// The maximum size of the small variant (mirror of `Small.Max`).
    pub const MAX: usize = SMALL_SIZE;

    /// Initializes the union for a given data type. This will
    /// attempt to fit into a small value if possible, otherwise
    /// will allocate and put into alloc.
    ///
    /// This can't and will never detect stable pointers.
    pub fn init(data: &[Elem]) -> Self {
        // If it fits in our small request, do that.
        if data.len() <= Self::MAX {
            let mut buf: [Elem; SMALL_SIZE] = [Elem::default(); SMALL_SIZE];
            buf[0..data.len()].copy_from_slice(data);
            return MessageData::Small(Small {
                data: buf,
                len: data.len(),
            });
        }

        // Otherwise, allocate
        let buf: Box<[Elem]> = data.to_vec().into_boxed_slice();
        MessageData::Alloc(Alloc { data: buf })
    }

    /// Returns a const slice of the data pointed to by this request.
    pub fn slice(&self) -> &[Elem] {
        match self {
            MessageData::Small(v) => &v.data[0..v.len],
            MessageData::Stable(v) => v,
            MessageData::Alloc(v) => &v.data,
        }
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    #[test]
    fn MessageData_init_small() {
        type Data<'a> = MessageData<'a, u8, 10>;
        let input = b"hello!";
        let io = Data::init(input.as_slice());
        assert!(matches!(io, MessageData::Small(_)));
    }

    #[test]
    fn MessageData_init_alloc() {
        type Data<'a> = MessageData<'a, u8, 10>;
        let input = "hello! ".repeat(100);
        let io = Data::init(input.as_bytes());
        assert!(matches!(io, MessageData::Alloc(_)));
        // io.alloc.alloc.free(io.alloc.data); -> handled by Drop in Rust.
        drop(io);
    }

    #[test]
    fn MessageData_small_fits_non_u8_sized_data() {
        const LEN: usize = 500;
        type Data<'a> = MessageData<'a, u8, LEN>;
        let input = "X".repeat(LEN);
        let io = Data::init(input.as_bytes());
        assert!(matches!(io, MessageData::Small(_)));
    }
}
