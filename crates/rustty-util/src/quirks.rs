//! Inspired by WebKit's quirks.cpp[1], this file centralizes all our
//! sad environment-specific hacks that we have to do to make things work.
//! This is a last resort; if we can find a general solution to a problem,
//! we of course prefer that, but sometimes other software, fonts, etc. are
//! just broken or weird and we have to work around it.
//!
//! [1]: https://github.com/WebKit/WebKit/blob/main/Source/WebCore/page/Quirks.cpp

// PORT/BREAK #2: the Zig reference imports `font/main.zig` here for the single
// font-dependent quirk `disableDefaultFontFeatures(face: *const font.Face)`.
// rustty-util is a leaf crate (R3/R4) and must NOT pull rustty-font in (it
// stays Miri-able). We therefore expose the font face as a trait seam (`Face`,
// below); the concrete font face type implements it and is wired in when
// rustty-font lands. No behavior is lost — the function's logic is ported as-is.

/// Seam for the font face type. In the Zig reference this is `font.Face`
/// (imported from `font/main.zig`). It is left abstract here so this leaf
/// crate does not depend on rustty-font; rustty-font's `Face` will implement
/// this trait to wire the quirk in.
pub trait Face {}

/// If true, the default font features should be disabled for the given face.
#[allow(non_snake_case)]
pub fn disableDefaultFontFeatures<F: Face + ?Sized>(face: &F) -> bool {
    let _ = face;

    // This function used to do something, but we integrated the logic
    // we checked for directly into our shaping algorithm. It's likely
    // there are other broken fonts for other reasons so I'm keeping this
    // around so its easy to add more checks in the future.
    false

    // var buf: [64]u8 = undefined;
    // const name = face.name(&buf) catch |err| switch (err) {
    //     // If the name doesn't fit in buf we know this will be false
    //     // because we have no quirks fonts that are longer than buf!
    //     error.OutOfMemory => return false,
    // };
}

/// We use our own assert function instead of `std.debug.assert`.
///
/// The only difference between this and the one in
/// the stdlib is that this version is marked inline.
///
/// The reason for this is that, despite the promises of the doc comment
/// on the stdlib function, the function call to `std.debug.assert` isn't
/// always optimized away in `ReleaseFast` mode, at least in Zig 0.15.2.
///
/// In the majority of places, the overhead from calling an empty function
/// is negligible, but we have some asserts inside tight loops and hotpaths
/// that cause significant overhead (as much as 15-20%) when they don't get
/// optimized out.
//
// In debug builds we just use std.debug.assert because this
// fixes up stack traces. `inline` causes broken stack traces. This
// is probably a Zig compiler bug but until it is fixed we have to
// do this for development sanity.
//
// .ReleaseSmall, .ReleaseSafe, .ReleaseFast => inline fn assert(ok: bool) void {
//     if (!ok) unreachable;
// }
//
// RUST CONTRACT (DG4 policy, PORT_RULES.md "DG2/DG4"; VALIDATION_REPORT §3d):
// the reference's `builtin.mode` switch makes this check ACTIVE in Debug and
// ReleaseSafe (`unreachable` traps there) and ELIDED in ReleaseFast/Small
// (`unreachable` is UB the optimizer removes — the quirk's whole purpose: the
// check must vanish from hotpaths in the shipped ReleaseFast artifact).
// `debug_assert!` reproduces that mode switch exactly: active under
// debug-assertions (dev profile + the `release-safe` parity profile the
// differential harness/CI use), compiled out in `release`/`release-fast`.
// Where the Zig elision is UB, the Rust elision is a defined no-op — strictly
// safer. Callers evaluate `ok` AT THE CALL SITE (a by-value fn argument), so
// argument side effects are preserved in every profile, exactly like the Zig
// `inline fn` whose argument is always evaluated. A previous draft used
// always-on `assert!`, inverting the quirk's ReleaseFast-elision purpose.
#[inline(always)]
#[allow(non_snake_case)]
pub fn inlineAssert(ok: bool) {
    debug_assert!(ok);
}

// rustty-only tests: lock the DG4 activation contract of `inlineAssert` in the
// lane where it is observable (`cargo test` runs with debug-assertions ON, the
// reference's Debug/ReleaseSafe behavior: a false assert traps).
#[cfg(test)]
mod inline_assert_tests {
    #[test]
    fn inline_assert_true_is_a_noop() {
        super::inlineAssert(true);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic]
    fn inline_assert_false_traps_in_checked_builds() {
        super::inlineAssert(false);
    }
}
