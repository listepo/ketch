// Smoke test: the app launches against a scratch KETCH_ROOT, shows its main
// window, lists the fake core's packages, and every sidebar item opens its
// screen. XCUITest has no Swift Testing equivalent, so this one file uses
// XCTest.

import XCTest

final class KetchUITests: XCTestCase {
    @MainActor
    func testLaunchesListsInstalledPackagesAndOpensEverySection() throws {
        let root = FileManager.default.temporaryDirectory
            .appending(path: "ketch-ui-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let app = XCUIApplication()
        app.launchEnvironment["KETCH_ROOT"] = root.path
        app.launch()
        defer { app.terminate() }

        XCTAssertTrue(app.windows.firstMatch.waitForExistence(timeout: 10))
        XCTAssertTrue(element(app, "sidebar-installed").waitForExistence(timeout: 5))
        // By identifier: a predicate over every element's label times out on CI runners.
        XCTAssertTrue(element(app, "installed-ripgrep").waitForExistence(timeout: 10))

        for section in ["discover", "updates", "activity", "doctor", "settings", "installed"] {
            element(app, "sidebar-\(section)").click()
            XCTAssertTrue(
                element(app, "page-\(section)").waitForExistence(timeout: 5), "\(section) did not open")
        }
    }

    @MainActor
    private func element(_ app: XCUIApplication, _ identifier: String) -> XCUIElement {
        app.descendants(matching: .any)[identifier].firstMatch
    }
}
