// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// Which upgrades count as new, what the notice says, and when the store posts
// one. The notifier is a recorder, so no permission dialog appears.

import Foundation
import Testing

@testable import Ketch

private final class RecordingNotifier: UpdateNotifier, @unchecked Sendable {
    private let lock = NSLock()
    private var _posts: [(title: String, body: String)] = []
    private var _authorizationRequests = 0
    let grants: Bool

    init(grants: Bool = true) { self.grants = grants }

    var posts: [(title: String, body: String)] { lock.withLock { _posts } }
    var authorizationRequests: Int { lock.withLock { _authorizationRequests } }

    func requestAuthorization() async -> Bool {
        lock.withLock { _authorizationRequests += 1 }
        return grants
    }

    func post(title: String, body: String) async {
        lock.withLock { _posts.append((title, body)) }
    }
}

private func upgrade(_ name: String, _ from: String, _ to: String, heldBy: String? = nil) -> Upgrade {
    Upgrade(name: name, from: from, to: to, heldBy: heldBy)
}

@Suite struct UpdateNoticesTests {
    @Test func anUpgradeNobodyAnnouncedIsNew() {
        let fresh = UpdateNotices.fresh([upgrade("rg", "1", "2")], notified: [])
        #expect(fresh.map(\.name) == ["rg"])
    }

    @Test func anUpgradeAlreadyAnnouncedIsNotNewAgain() {
        let fresh = UpdateNotices.fresh([upgrade("rg", "1", "2")], notified: ["rg@2"])
        #expect(fresh.isEmpty)
    }

    @Test func aNewerReleaseOfAnAnnouncedPackageIsNewAgain() {
        let fresh = UpdateNotices.fresh([upgrade("rg", "1", "3")], notified: ["rg@2"])
        #expect(fresh.map(\.to) == ["3"])
    }

    @Test func aHeldPackageNeverCountsBecauseUpgradeWouldLeaveItAlone() {
        let fresh = UpdateNotices.fresh([upgrade("jq", "1", "2", heldBy: "ketch.lock")], notified: [])
        #expect(fresh.isEmpty)
    }

    @Test func theNoticeNamesAtMostThreePackagesAndCountsTheRest() {
        let many = (1...5).map { upgrade("p\($0)", "1", "2") }
        let message = UpdateNotices.message(for: many)
        #expect(message.title == "5 updates available")
        #expect(message.body == "p1 1 → 2\np2 1 → 2\np3 1 → 2\nand 2 more")
    }

    @Test func aSingleUpdateReadsInTheSingular() {
        #expect(UpdateNotices.message(for: [upgrade("rg", "1", "2")]).title == "1 update available")
    }

    @Test func controlAndBidiCharactersFromAReleaseTagAreDropped() {
        let message = UpdateNotices.message(for: [upgrade("rg", "1", "2\u{1b}[31m\u{202e}x")])
        #expect(message.body == "rg 1 → 2[31mx")
    }
}

@MainActor
@Suite struct UpdateNotificationStoreTests {
    private func makeStore(
        _ notifier: RecordingNotifier, notifies: Bool = true
    ) -> KetchStore {
        let settings = AppSettings(defaults: scratchDefaults())
        settings.notifiesOfUpdates = notifies
        return KetchStore(core: FakeKetchCore(), settings: settings, notifier: notifier)
    }

    @Test func severalNewUpgradesPostOneNotice() async {
        let notifier = RecordingNotifier()
        let store = makeStore(notifier)
        await store.refresh()

        await store.notifyOfNewUpdates()

        #expect(notifier.posts.count == 1)
        #expect(notifier.posts.first?.title == "\(store.updates.count) updates available")
    }

    @Test func theSameUpgradesAreNotAnnouncedTwice() async {
        let notifier = RecordingNotifier()
        let store = makeStore(notifier)
        await store.refresh()

        await store.notifyOfNewUpdates()
        await store.notifyOfNewUpdates()

        #expect(notifier.posts.count == 1)
    }

    @Test func nothingIsPostedWhileNotificationsAreOff() async {
        let notifier = RecordingNotifier()
        let store = makeStore(notifier, notifies: false)
        await store.refresh()

        await store.notifyOfNewUpdates()

        #expect(notifier.posts.isEmpty)
    }

    @Test func heldPackagesAreLeftOutOfTheNotice() async {
        let notifier = RecordingNotifier()
        let store = makeStore(notifier)
        await store.refresh()
        #expect(store.pinned.map(\.name) == ["jq"])

        await store.notifyOfNewUpdates()

        #expect(notifier.posts.first?.body.contains("jq") == false)
    }

    @Test func permissionIsAskedOnlyWhenTheUserTurnsNotificationsOn() async {
        let notifier = RecordingNotifier()
        let store = makeStore(notifier, notifies: false)
        await store.refresh()
        await store.notifyOfNewUpdates()
        #expect(notifier.authorizationRequests == 0)

        let message = await store.setNotifications(true)

        #expect(message == nil)
        #expect(notifier.authorizationRequests == 1)
        #expect(store.settings.notifiesOfUpdates)
    }

    @Test func aRefusalLeavesNotificationsOffAndSaysWhy() async {
        let store = makeStore(RecordingNotifier(grants: false), notifies: false)

        let message = await store.setNotifications(true)

        #expect(message?.contains("System Settings") == true)
        #expect(!store.settings.notifiesOfUpdates)
    }

    @Test func turningNotificationsOffAsksForNothing() async {
        let notifier = RecordingNotifier()
        let store = makeStore(notifier)

        _ = await store.setNotifications(false)

        #expect(notifier.authorizationRequests == 0)
        #expect(!store.settings.notifiesOfUpdates)
    }

    @Test func clickingTheNoticeRequestsUpdatesAndTheWindow() {
        let store = makeStore(RecordingNotifier())

        store.openUpdates()

        #expect(store.requestedSection == .updates)
        #expect(store.windowRequests == 1)
    }

    @Test func announcedUpgradesSurviveARestart() async {
        let defaults = scratchDefaults()
        let first = AppSettings(defaults: defaults)
        first.notifiesOfUpdates = true
        let store = KetchStore(core: FakeKetchCore(), settings: first, notifier: RecordingNotifier())
        await store.refresh()
        await store.notifyOfNewUpdates()

        let reloaded = AppSettings(defaults: defaults)

        #expect(reloaded.notifiedUpgrades == first.notifiedUpgrades)
        #expect(!reloaded.notifiedUpgrades.isEmpty)
    }
}
