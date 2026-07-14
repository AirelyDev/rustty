//
//  RusttyTitleUITests.swift
//  RusttyUITests
//
//  Created by luca on 13.10.2025.
//

import XCTest

final class RusttyTitleUITests: RusttyCustomConfigCase {
    override func setUp() async throws {
        try await super.setUp()
        try updateConfig(#"title = "RusttyUITestsLaunchTests""#)
    }

    @MainActor
    func testTitle() throws {
        let app = try rusttyApplication()
        app.launch()

        XCTAssertEqual(app.windows.firstMatch.title, "RusttyUITestsLaunchTests", "Oops, `title=` doesn't work!")
    }
}
