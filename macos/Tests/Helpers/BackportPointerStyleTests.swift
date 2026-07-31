import Testing
import SwiftUI
@testable import Rustty

#if canImport(AppKit)

/// Asserts the `BackportPointerStyle.official` mapping introduced by ghostty
/// `fa3802a70` ("macOS: change split drag's point style to match HIG"), which
/// replaced the six `frameResize(position:directions:)` spellings with
/// `columnResize(directions:)` / `rowResize(directions:)`.
///
/// Both spellings are macOS 15 SwiftUI APIs, so this mapping is assertable on
/// any macOS 15+ host — it is NOT macOS-27-gated. (Only the *rendered cursor
/// artwork* differs on macOS 27; that remains a human visual judgement and is
/// explicitly out of scope here.)
///
/// `PointerStyle` exposes no `Equatable` conformance and no introspection, so
/// the comparison goes through `String(describing:)`. That it discriminates all
/// six cases is not assumed — `testResizeCaseDescriptionsArePairwiseDistinct`
/// below proves it, so a swapped or mis-spelled case cannot pass vacuously.
struct BackportPointerStyleTests {
    /// The six resize cases and the `PointerStyle` each must map to, written out
    /// independently of `Backport.swift`.
    @available(macOS 15, *)
    private static func resizeExpectations() -> [(BackportPointerStyle, PointerStyle, String)] {
        [
            (.resizeLeft, .columnResize(directions: .leading), "resizeLeft"),
            (.resizeRight, .columnResize(directions: .trailing), "resizeRight"),
            (.resizeUp, .rowResize(directions: .up), "resizeUp"),
            (.resizeDown, .rowResize(directions: .down), "resizeDown"),
            (.resizeUpDown, .rowResize, "resizeUpDown"),
            (.resizeLeftRight, .columnResize, "resizeLeftRight"),
        ]
    }

    @Test func testResizeCasesMapToColumnAndRowResize() throws {
        guard #available(macOS 15, *) else {
            Issue.record("BackportPointerStyle.official requires macOS 15+; host is too old to assert")
            return
        }
        for (backport, expected, label) in Self.resizeExpectations() {
            let actual = String(describing: backport.official)
            #expect(
                actual == String(describing: expected),
                "\(label): expected \(String(describing: expected)), got \(actual)")
        }
    }

    /// `fa3802a70` removed `frameResize` entirely from this mapping.
    @Test func testNoResizeCaseStillUsesFrameResize() throws {
        guard #available(macOS 15, *) else {
            Issue.record("BackportPointerStyle.official requires macOS 15+; host is too old to assert")
            return
        }
        for (backport, _, label) in Self.resizeExpectations() {
            let actual = String(describing: backport.official)
            #expect(!actual.contains("frameResize"), "\(label): still uses frameResize — \(actual)")
        }
    }

    /// Guards the discriminating power of `String(describing:)`: if any two of
    /// the six descriptions collided, the assertions above could pass with the
    /// mapping wrong.
    @Test func testResizeCaseDescriptionsArePairwiseDistinct() throws {
        guard #available(macOS 15, *) else {
            Issue.record("BackportPointerStyle.official requires macOS 15+; host is too old to assert")
            return
        }
        let descriptions = Self.resizeExpectations().map { String(describing: $0.1) }
        #expect(Set(descriptions).count == descriptions.count, "descriptions collide: \(descriptions)")
    }

    /// The six non-resize cases `fa3802a70` left untouched, so a future edit to
    /// this switch cannot silently disturb them.
    @Test func testNonResizeCasesAreUnchanged() throws {
        guard #available(macOS 15, *) else {
            Issue.record("BackportPointerStyle.official requires macOS 15+; host is too old to assert")
            return
        }
        let expectations: [(BackportPointerStyle, PointerStyle, String)] = [
            (.default, .default, "default"),
            (.grabIdle, .grabIdle, "grabIdle"),
            (.grabActive, .grabActive, "grabActive"),
            (.horizontalText, .horizontalText, "horizontalText"),
            (.verticalText, .verticalText, "verticalText"),
            (.link, .link, "link"),
        ]
        for (backport, expected, label) in expectations {
            let actual = String(describing: backport.official)
            #expect(
                actual == String(describing: expected),
                "\(label): expected \(String(describing: expected)), got \(actual)")
        }
    }
}

#endif
