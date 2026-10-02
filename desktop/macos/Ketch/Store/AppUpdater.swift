// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

// The app's own updates, through Sparkle. Separate from `KetchStore`, which
// checks the packages ketch manages; this is the one thing that updates
// Ketch.app itself, from the appcast the release workflow publishes.

import Combine
import Foundation
import Observation
import Sparkle

@MainActor
@Observable
final class AppUpdater {
    /// The value `SUPublicEDKey` holds until the creator generates the real
    /// key; the release workflow refuses to ship it.
    nonisolated static let placeholderKey = "SPARKLE_ED_PUBLIC_KEY_PLACEHOLDER"

    /// Whether "Check for Updates…" can run now: false while a check is in
    /// flight, and always false when the updater is not started.
    private(set) var canCheckForUpdates = false

    @ObservationIgnored private let controller: SPUStandardUpdaterController?
    @ObservationIgnored private var observation: AnyCancellable?

    init(bundle: Bundle = .main, environment: [String: String] = ProcessInfo.processInfo.environment) {
        #if DEBUG
            let isDebug = true
        #else
            let isDebug = false
        #endif
        let start = Self.shouldStart(
            publicKey: bundle.object(forInfoDictionaryKey: "SUPublicEDKey") as? String,
            isDebug: isDebug,
            underTests: environment["XCTestConfigurationFilePath"] != nil)
        guard start else {
            controller = nil
            return
        }
        let controller = SPUStandardUpdaterController(
            startingUpdater: true, updaterDelegate: nil, userDriverDelegate: nil)
        self.controller = controller
        // KVO through Combine, as Sparkle's SwiftUI guide does; SPUUpdater
        // changes the property on the main thread.
        observation = controller.updater.publisher(for: \.canCheckForUpdates)
            .receive(on: DispatchQueue.main)
            .sink { [weak self] value in self?.canCheckForUpdates = value }
    }

    func checkForUpdates() {
        controller?.checkForUpdates(nil)
    }

    /// Only a release build with a real key starts Sparkle. A Debug build
    /// would offer the published release to a development copy, a
    /// placeholder key makes Sparkle refuse to start with an alert on every
    /// launch, and a hosted test run must not reach the network.
    nonisolated static func shouldStart(publicKey: String?, isDebug: Bool, underTests: Bool) -> Bool {
        guard let publicKey, !publicKey.isEmpty, publicKey != placeholderKey else { return false }
        return !isDebug && !underTests
    }
}
