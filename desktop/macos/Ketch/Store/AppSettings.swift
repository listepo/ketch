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
    }

    @ObservationIgnored private let defaults: UserDefaults

    /// Minutes between background update checks.
    var updateCheckIntervalMinutes: Int {
        didSet { defaults.set(updateCheckIntervalMinutes, forKey: Key.interval) }
    }

    var includePrereleases: Bool {
        didSet { defaults.set(includePrereleases, forKey: Key.prereleases) }
    }

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        let stored = defaults.integer(forKey: Key.interval)
        updateCheckIntervalMinutes = stored > 0 ? stored : 60
        includePrereleases = defaults.bool(forKey: Key.prereleases)
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
