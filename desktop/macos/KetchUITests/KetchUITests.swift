// Smoke test: the app launches against a scratch KETCH_ROOT, shows its main
// window, and lists the fake core's packages. XCUITest has no Swift Testing
// equivalent, so this one file uses XCTest.

import XCTest

final class KetchUITests: XCTestCase {
    @MainActor
    func testLaunchesAndListsInstalledPackages() throws {
        let root = FileManager.default.temporaryDirectory
            .appending(path: "ketch-ui-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let app = XCUIApplication()
        app.launchEnvironment["KETCH_ROOT"] = root.path
        app.launch()
        defer { app.terminate() }

        XCTAssertTrue(app.windows.firstMatch.waitForExistence(timeout: 10))
        XCTAssertTrue(app.descendants(matching: .any)["sidebar-installed"].waitForExistence(timeout: 5))
        // A List row merges its texts into one element, so match on the label.
        let ripgrep = app.descendants(matching: .any)
            .matching(NSPredicate(format: "label CONTAINS 'ripgrep' OR value CONTAINS 'ripgrep'")).firstMatch
        XCTAssertTrue(ripgrep.waitForExistence(timeout: 5), app.debugDescription)
    }
}
