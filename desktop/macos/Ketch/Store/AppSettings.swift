// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The app's own preferences, in UserDefaults. Anything ketch itself is
// configured by stays in the CLI's config.toml: the app never stores tokens
// or a second copy of ketch's settings.

import Foundation
import Observation
import ServiceManagement

@MainActor
@Observable
final class AppSettings {
    /// The choices offered for the update check, in minutes.
    static let intervals = [15, 60, 360, 1440]

    private enum Key {
        static let interval = "updateCheckIntervalMinutes"
        static let prereleases = "includePrereleases"
        static let notifies = "notifiesOfUpdates"
        static let notified = "notifiedUpgrades"
        static let tint = "appearanceTint"
        static let glassStyle = "appearanceGlassStyle"
        static let accent = "appearanceAccent"
        static let wash = "appearanceWash"
    }

    @ObservationIgnored private let defaults: UserDefaults

    /// Minutes between background update checks.
    var updateCheckIntervalMinutes: Int {
        didSet { defaults.set(updateCheckIntervalMinutes, forKey: Key.interval) }
    }

    var includePrereleases: Bool {
        didSet { defaults.set(includePrereleases, forKey: Key.prereleases) }
    }

    /// Whether a new upgrade posts a system notification. Off until the user
    /// turns it on, which is also when macOS is asked for permission.
    var notifiesOfUpdates: Bool {
        didSet { defaults.set(notifiesOfUpdates, forKey: Key.notifies) }
    }

    /// `UpdateNotices.key`s already announced, so a check that finds the same
    /// upgrades again stays quiet.
    var notifiedUpgrades: Set<String> {
        didSet { defaults.set(Array(notifiedUpgrades).sorted(), forKey: Key.notified) }
    }

    /// Settings -> Appearance, as the user chose it. Views draw
    /// `Appearance.resolved(…)`, never this directly.
    var appearance: Appearance {
        didSet {
            defaults.set(appearance.tint.stored, forKey: Key.tint)
            defaults.set(appearance.glassStyle.rawValue, forKey: Key.glassStyle)
            defaults.set(appearance.accent.stored, forKey: Key.accent)
            defaults.set(Appearance.clampWash(appearance.wash), forKey: Key.wash)
        }
    }

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        let stored = defaults.integer(forKey: Key.interval)
        updateCheckIntervalMinutes = stored > 0 ? stored : 60
        includePrereleases = defaults.bool(forKey: Key.prereleases)
        notifiesOfUpdates = defaults.bool(forKey: Key.notifies)
        notifiedUpgrades = Set(defaults.stringArray(forKey: Key.notified) ?? [])
        appearance = Appearance(
            tint: TintChoice(stored: defaults.string(forKey: Key.tint)),
            glassStyle: defaults.string(forKey: Key.glassStyle).flatMap(GlassStyle.init(rawValue:)) ?? .regular,
            accent: AccentChoice(stored: defaults.string(forKey: Key.accent)),
            wash: Appearance.clampWash((defaults.object(forKey: Key.wash) as? Double) ?? Tokens.Opacity.wash)
        )
    }

    var updateCheckInterval: Duration { .seconds(updateCheckIntervalMinutes * 60) }

    // MARK: Open at login

    /// Whether the app is registered as a login item. Read from the system
    /// each time rather than stored, since the user can change it in System
    /// Settings.
    var opensAtLogin: Bool { SMAppService.mainApp.status == .enabled }

    /// Registers or unregisters the login item; returns the error message on failure.
    func setOpensAtLogin(_ enabled: Bool) -> String? {
        do {
            if enabled {
                try SMAppService.mainApp.register()
            } else {
                try SMAppService.mainApp.unregister()
            }
            return nil
        } catch {
            return error.localizedDescription
        }
    }
}
