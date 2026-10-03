// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// Which upgrades are worth a notification, what it says, and the system
// notification that carries it. The rule is pure so a test can pin it; the
// store decides when to ask and the notifier owns everything that touches
// `UserNotifications`.

import Foundation
import UserNotifications

enum UpdateNotices {
    /// An upgrade is identified by its package and target version: a package
    /// that moves on to a newer release is news again, one that stays put is not.
    static func key(_ upgrade: Upgrade) -> String { "\(upgrade.name)@\(upgrade.to)" }

    /// The upgrades `notified` has not announced yet. Held packages never
    /// count: `upgrade` would leave them where they are, so telling the user
    /// about them is a promise the app cannot keep.
    static func fresh(_ upgrades: [Upgrade], notified: Set<String>) -> [Upgrade] {
        upgrades.filter { $0.heldBy == nil && !notified.contains(key($0)) }
    }

    /// The notification text. Versions come from release tags, which a
    /// project's author wrote, so control characters are dropped before the
    /// text reaches the system.
    static func message(for upgrades: [Upgrade], listed: Int = 3) -> (title: String, body: String) {
        let title = upgrades.count == 1 ? "1 update available" : "\(upgrades.count) updates available"
        let shown = upgrades.prefix(listed).map { printable("\($0.name) \($0.from) → \($0.to)") }
        let more = upgrades.count > listed ? ["and \(upgrades.count - listed) more"] : []
        return (title, (shown + more).joined(separator: "\n"))
    }

    static func printable(_ text: String) -> String {
        String(text.unicodeScalars.filter { !$0.properties.generalCategory.isControlOrFormat })
    }
}

extension Unicode.GeneralCategory {
    /// Control characters and format characters such as the bidi overrides.
    fileprivate var isControlOrFormat: Bool { self == .control || self == .format }
}

/// The part of `UserNotifications` the app uses, so tests need no system
/// permission dialog.
protocol UpdateNotifier: Sendable {
    /// Asks macOS for permission to alert; `true` when it is granted.
    func requestAuthorization() async -> Bool
    func post(title: String, body: String) async
}

/// Posts through `UNUserNotificationCenter`. It does nothing outside an app
/// bundle, where `current()` would trap.
struct SystemUpdateNotifier: UpdateNotifier {
    private var center: UNUserNotificationCenter? {
        Bundle.main.bundleURL.pathExtension == "app" ? .current() : nil
    }

    func requestAuthorization() async -> Bool {
        guard let center else { return false }
        return (try? await center.requestAuthorization(options: [.alert])) ?? false
    }

    func post(title: String, body: String) async {
        guard let center else { return }
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        // One slot: a newer notice replaces an unread older one instead of stacking.
        let request = UNNotificationRequest(identifier: "ketch.updates", content: content, trigger: nil)
        try? await center.add(request)
    }
}

/// Sends a click on the notification to the app, which opens Updates. It is
/// the center's delegate for the life of the process; `onOpen` is set once the
/// store exists.
final class NotificationRouter: NSObject, UNUserNotificationCenterDelegate, @unchecked Sendable {
    static let shared = NotificationRouter()

    @MainActor var onOpen: (() -> Void)?

    func install() {
        guard Bundle.main.bundleURL.pathExtension == "app" else { return }
        UNUserNotificationCenter.current().delegate = self
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse
    ) async {
        guard response.actionIdentifier == UNNotificationDefaultActionIdentifier else { return }
        await MainActor.run { onOpen?() }
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter, willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        // The app is frontmost: Updates is already showing the news.
        []
    }
}
