// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// When the app starts Sparkle, and that the shipped Info.plist carries the
// keys the release workflow and the updater depend on.

import Foundation
import Testing

@testable import Ketch

@Suite struct AppUpdaterTests {
    private let realKey = "pbDYEm7UsTZ3dNvf3kIHKpkRQ2sNe0CpqSe5rLhNk0M="

    @Test func aReleaseBuildWithARealKeyStartsTheUpdater() {
        #expect(AppUpdater.shouldStart(publicKey: realKey, isDebug: false, underTests: false))
    }

    @Test(arguments: [nil, "", AppUpdater.placeholderKey])
    func aMissingOrPlaceholderKeyNeverStartsTheUpdater(key: String?) {
        #expect(!AppUpdater.shouldStart(publicKey: key, isDebug: false, underTests: false))
    }

    @Test func debugBuildsAndTestRunsNeverStartTheUpdater() {
        #expect(!AppUpdater.shouldStart(publicKey: realKey, isDebug: true, underTests: false))
        #expect(!AppUpdater.shouldStart(publicKey: realKey, isDebug: false, underTests: true))
    }

    @MainActor
    @Test func theHostedTestRunLeavesCheckForUpdatesDisabled() {
        #expect(!AppUpdater().canCheckForUpdates)
    }

    /// The feed must not be /releases/latest: that URL is the CLI's, and app
    /// releases are never marked latest.
    @Test func theAppcastIsTheStableDesktopFeed() throws {
        // Unit tests are hosted in Ketch.app, so the main bundle is the app.
        let bundle = Bundle.main
        let feed = try #require(bundle.object(forInfoDictionaryKey: "SUFeedURL") as? String)
        #expect(feed == "https://github.com/pyrlyn/ketch/releases/download/desktop-appcast/appcast.xml")
        #expect(bundle.object(forInfoDictionaryKey: "SURequireSignedFeed") as? Bool == true)
        #expect(bundle.object(forInfoDictionaryKey: "SUPublicEDKey") is String)
    }
}
