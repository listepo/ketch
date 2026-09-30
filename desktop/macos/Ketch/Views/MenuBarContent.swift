// The menu-bar extra: pending upgrades, Upgrade all, the running operation,
// Open and Quit, as a glass panel (`.menuBarExtraStyle(.window)`) rather than
// a plain menu, so it can show progress and match the main window.

import SwiftUI

struct MenuBarLabel: View {
    let count: Int
    let isRunning: Bool

    var body: some View {
        let symbol = isRunning ? "arrow.down.circle" : "shippingbox"
        if count > 0 {
            Label("\(count)", systemImage: symbol).labelStyle(.titleAndIcon)
        } else {
            Image(systemName: symbol)
        }
    }
}

struct MenuBarContent: View {
    @Environment(KetchStore.self) private var store
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        GlassEffectContainer(spacing: Theme.Spacing.cards) {
            VStack(alignment: .leading, spacing: Theme.Spacing.cards) {
                HStack {
                    Image(systemName: "shippingbox.fill").font(.title2).foregroundStyle(.tint)
                    VStack(alignment: .leading) {
                        Text("Ketch").font(.headline)
                        Text(summary).font(.caption).foregroundStyle(.secondary)
                    }
                }
                .glassCard()

                if let activity = store.activity {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(activity.title).font(.subheadline)
                        ProgressView(value: activity.fraction)
                        Button("Cancel") { store.cancel() }
                            .buttonStyle(.glass)
                            .disabled(activity.isCancelling)
                    }
                    .glassCard()
                }

                // The confirmation is the main window's; the panel only asks for it.
                Button {
                    openMain()
                    store.confirmingUpgradeAll = true
                } label: {
                    Label("Upgrade All…", systemImage: "arrow.up.circle").frame(maxWidth: .infinity)
                }
                .buttonStyle(.glassProminent)
                .disabled(store.pendingUpgradeCount == 0 || store.isRunning)

                HStack {
                    Button("Check Now", systemImage: "arrow.clockwise") { Task { await store.refresh() } }
                        .disabled(store.isRunning)
                    Button("Open Ketch", systemImage: "macwindow") { openMain() }
                }
                .buttonStyle(.glass)

                HStack {
                    SettingsLink { Label("Settings…", systemImage: "gearshape") }
                        .buttonStyle(.glass)
                    Spacer()
                    Button("Quit", systemImage: "power") { NSApplication.shared.terminate(nil) }
                        .buttonStyle(.glass)
                        .keyboardShortcut("q")
                }
            }
            .padding(Theme.Spacing.cardPadding)
        }
        .frame(width: 280)
        .background(Backdrop())
    }

    private var summary: String {
        switch store.pendingUpgradeCount {
        case 0: "Everything is up to date"
        case 1: "1 upgrade pending"
        case let count: "\(count) upgrades pending"
        }
    }

    private func openMain() {
        openWindow(id: WindowID.main)
        NSApplication.shared.activate()
    }
}
