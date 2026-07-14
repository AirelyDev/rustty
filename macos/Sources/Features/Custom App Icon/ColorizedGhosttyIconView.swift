import SwiftUI
import Cocoa

// For testing.
struct ColorizedRusttyIconView: View {
    var body: some View {
        Image(nsImage: ColorizedRusttyIcon(
            screenColors: [.purple, .blue],
            ghostColor: .yellow,
            frame: .aluminum
        ).makeImage(in: .main)!)
    }
}
