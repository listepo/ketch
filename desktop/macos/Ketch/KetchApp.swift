// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The app's scenes: the main window, the menu-bar extra, Settings and About.
// One `KetchStore` serves all of them, so the menu bar and the window always
// agree on what is installed and what is running.

import SwiftUI

@main
struct KetchApp: App {
    @State private var settings: AppSettings
    @State private var store: KetchStore
    @State private var updater = AppUpdater()

    init() {
        let settings = AppSettings()
        let store = KetchStore(core: CoreFactory.make(), settings: settings)
        _settings = State(initialValue: settings)
        _store = State(initialValue: store)
        // A hosted unit-test run launches the app too; its timer would race
        // the tests' own store.
        if ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] == nil {
            store.startUpdateChecks()
        }
    }

    var body: some Scene {
        Window("Ketch", id: WindowID.main) {
            ContentView()
                .frame(minWidth: 760, minHeight: 480)
                .ketchAppearance()
                .environment(store)
                .environment(settings)
        }
        .commands { AppCommands(updater: updater) }

        MenuBarExtra {
            MenuBarContent()
                .ketchAppearance()
                .environment(store)
                .environment(settings)
        } label: {
            MenuBarLabel(count: store.pendingUpgradeCount, isRunning: store.isRunning)
        }
        .menuBarExtraStyle(.window)

        Settings {
            SettingsView()
                .frame(width: 520)
                .ketchAppearance()
                .environment(store)
                .environment(settings)
        }

        Window("About Ketch", id: WindowID.about) {
            AboutView()
        }
        .windowResizability(.contentSize)
    }
}

enum WindowID {
    static let main = "main"
    static let about = "about"
}

private struct AppCommands: Commands {
    let updater: AppUpdater
    @Environment(\.openWindow) private var openWindow

    var body: some Commands {
        CommandGroup(replacing: .appInfo) {
            Button("About Ketch") { openWindow(id: WindowID.about) }
            CheckForUpdatesButton(updater: updater)
        }
    }
}

/// A view rather than a bare `Button` in the command group, so the menu item
/// re-reads `canCheckForUpdates` when Sparkle changes it.
private struct CheckForUpdatesButton: View {
    let updater: AppUpdater

    var body: some View {
        Button("Check for Updates…") { updater.checkForUpdates() }
            .disabled(!updater.canCheckForUpdates)
    }
}
