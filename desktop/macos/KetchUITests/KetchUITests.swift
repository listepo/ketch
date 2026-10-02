// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

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

    /// A control with no label is silent to VoiceOver, which `performAccessibilityAudit`
    /// reports as an insufficient element description. This stands in for a
    /// VoiceOver walk through the screens; it cannot judge whether a label is good.
    @MainActor
    func testEveryScreenPassesTheAccessibilityLabelAudit() throws {
        let root = FileManager.default.temporaryDirectory
            .appending(path: "ketch-ui-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let app = XCUIApplication()
        app.launchEnvironment["KETCH_ROOT"] = root.path
        app.launch()
        defer { app.terminate() }

        XCTAssertTrue(element(app, "installed-ripgrep").waitForExistence(timeout: 10))
        // The menu-bar extra is a symbol and a number; VoiceOver needs the sentence.
        XCTAssertTrue(
            app.statusItems["Ketch, 2 updates available"].waitForExistence(timeout: 5),
            "the menu-bar extra is unlabeled")

        for section in ["installed", "discover", "updates", "activity", "doctor", "settings"] {
            element(app, "sidebar-\(section)").click()
            XCTAssertTrue(element(app, "page-\(section)").waitForExistence(timeout: 5), "\(section) did not open")
            try audit(app, screen: section)
        }

        // Controls the audit cannot reach on a quiet screen, found by the label
        // VoiceOver reads: a symbol-only button is found by its title only if it has one.
        element(app, "sidebar-settings").click()
        app.tabs["Appearance"].click()
        try audit(app, screen: "appearance settings")
        for title in ["Custom tint", "Custom accent"] {
            let well = app.colorWells.matching(NSPredicate(format: "label BEGINSWITH %@", title)).firstMatch
            XCTAssertTrue(well.waitForExistence(timeout: 5), "\(title) colour well is unlabeled")
        }

        element(app, "sidebar-installed").click()
        app.buttons["Upgrade all 2"].click()
        app.sheets.buttons["Update All"].firstMatch.click()
        XCTAssertTrue(app.buttons["Cancel"].waitForExistence(timeout: 10), "cancel button is unlabeled")
        let finished = NSPredicate(format: "exists == false")
        expectation(for: finished, evaluatedWith: app.buttons["Cancel"])
        waitForExpectations(timeout: 60)

        element(app, "installed-ripgrep").click()
        XCTAssertTrue(app.buttons["Uninstall"].waitForExistence(timeout: 5))
        try audit(app, screen: "package detail")
        app.buttons["Uninstall"].click()
        XCTAssertTrue(app.buttons["Cancel"].waitForExistence(timeout: 5))
        try audit(app, screen: "uninstall sheet")
    }

    /// Layout containers have no description of their own; only an element the
    /// user can read, focus or act on needs one.
    private static let containers: Set<XCUIElement.ElementType> = [
        .group, .splitGroup, .scrollView, .outline, .outlineRow, .cell, .tableColumn, .splitter,
        .window, .other, .scrollBar, .layoutArea, .layoutItem, .touchBar,
        // A slider's thumb is part of the slider, which carries the label and value.
        .valueIndicator,
    ]

    @MainActor
    private func audit(_ app: XCUIApplication, screen: String) throws {
        var findings: [String] = []
        try app.performAccessibilityAudit(for: [.sufficientElementDescription, .elementDetection]) { issue in
            guard let element = issue.element, !Self.containers.contains(element.elementType),
                // The Touch Bar's emoji button hangs off the window while a text field has focus.
                app.windows.firstMatch.frame.intersects(element.frame)
            else { return true }
            findings.append(
                "\(issue.compactDescription): \(element.elementType.rawValue) id '\(element.identifier)' frame \(element.frame)"
            )
            return true
        }
        XCTAssertTrue(findings.isEmpty, "\(screen) has unlabeled controls:\n" + findings.joined(separator: "\n"))
    }

    @MainActor
    private func element(_ app: XCUIApplication, _ identifier: String) -> XCUIElement {
        app.descendants(matching: .any)[identifier].firstMatch
    }
}
