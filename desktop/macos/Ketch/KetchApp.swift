// The app's scenes: the main window, the menu-bar extra, Settings and About.
// One `KetchStore` serves all of them, so the menu bar and the window always
// agree on what is installed and what is running.

import SwiftUI

@main
struct KetchApp: App {
    @State private var settings: AppSettings
    @State private var store: KetchStore

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
                .environment(store)
                .environment(settings)
                .frame(minWidth: 760, minHeight: 480)
        }
        .commands { AppCommands() }

        MenuBarExtra {
            MenuBarContent()
                .environment(store)
        } label: {
            MenuBarLabel(count: store.pendingUpgradeCount, isRunning: store.isRunning)
        }
        .menuBarExtraStyle(.window)

        Settings {
            SettingsView()
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
    @Environment(\.openWindow) private var openWindow

    var body: some Commands {
        CommandGroup(replacing: .appInfo) {
            Button("About Ketch") { openWindow(id: WindowID.about) }
        }
    }
}
