// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// Settings: General (update checks, prereleases, open at login, and where
// ketch's own configuration lives) and Appearance (AppearanceSettingsView).

import SwiftUI

struct SettingsView: View {
    var body: some View {
        TabView {
            Tab("General", systemImage: "gearshape") { GeneralSettingsView() }
            Tab("Appearance", systemImage: "paintpalette") { AppearanceSettingsView() }
        }
        // Shown in the Settings window and as the main window's Settings
        // section, where the backdrop shows through the grouped forms.
        .scrollContentBackground(.hidden)
    }
}

private struct GeneralSettingsView: View {
    @Environment(KetchStore.self) private var store
    @Environment(AppSettings.self) private var settings
    @State private var opensAtLogin = false
    @State private var message: String?

    var body: some View {
        @Bindable var settings = settings
        Form {
            Picker("Check for updates", selection: $settings.updateCheckIntervalMinutes) {
                ForEach(AppSettings.intervals, id: \.self) { minutes in
                    Text(label(minutes)).tag(minutes)
                }
            }
            Toggle("Include prereleases when installing", isOn: $settings.includePrereleases)
            Toggle("Open at login", isOn: $opensAtLogin)
                .onChange(of: opensAtLogin) { _, enabled in
                    guard enabled != settings.opensAtLogin else { return }
                    message = settings.setOpensAtLogin(enabled)
                    opensAtLogin = settings.opensAtLogin
                }

            LabeledContent("ketch root") {
                Text(store.root.path).textSelection(.enabled).monospaced()
            }
            Text("Shared with the ketch CLI. Set KETCH_ROOT in the app's environment to use another root.")
                .font(.caption).foregroundStyle(.secondary)
            Button("Open config.toml") { openConfig() }
            Text(
                "The app stores no tokens. ketch reads a GitHub token from its config.toml or KETCH_GITHUB_TOKEN, GITHUB_TOKEN or GH_TOKEN, as the CLI does."
            )
            .font(.caption).foregroundStyle(.secondary)

            if let message {
                Text(message).foregroundStyle(Theme.Palette.error)
            }
        }
        .formStyle(.grouped)
        .onAppear { opensAtLogin = settings.opensAtLogin }
        .onChange(of: settings.updateCheckIntervalMinutes) { store.startUpdateChecks() }
    }

    private func label(_ minutes: Int) -> String {
        switch minutes {
        case ..<60: "Every \(minutes) minutes"
        case 60: "Every hour"
        case 1440: "Every day"
        default: "Every \(minutes / 60) hours"
        }
    }

    private func openConfig() {
        let file = KetchRoot.configFile(in: store.root)
        if FileManager.default.fileExists(atPath: file.path) {
            NSWorkspace.shared.open(file)
        } else {
            message = "\(file.path) does not exist yet; ketch runs on its defaults until it does."
        }
    }
}
