// Updates: every installed package with a newer release, each with its own
// Update and Update all in the header, then the packages a ketch.lock holds
// back. The busy banner above the list (from `Page`) is where a refused
// update says who holds the lock.

import SwiftUI

struct UpdatesView: View {
    @Environment(KetchStore.self) private var store

    var body: some View {
        Page(title: "Updates", subtitle: subtitle) {
            Button("Check now") { Task { await store.refresh() } }
                .buttonStyle(.glass)
                .disabled(store.isRunning)
            Button("Update all") { store.confirmingUpgradeAll = true }
                .buttonStyle(.glassProminent)
                .disabled(store.pendingUpgradeCount == 0 || store.isRunning)
                .accessibilityIdentifier("updates-all")
        } content: {
            PageScroll {
                ForEach(store.updates) { update in
                    PackageRow(name: update.name, subtitle: "\(update.from) → \(update.to)") {
                        Button("Update") { Task { await store.upgrade([update.name]) } }
                            .buttonStyle(.tint)
                            .disabled(store.isRunning)
                            .accessibilityLabel("Update \(update.name)")
                            .accessibilityIdentifier("update-\(update.name)")
                    }
                }
                if !store.pinned.isEmpty {
                    PinnedCard(pinned: store.pinned)
                        .padding(.top, Tokens.Space.xs)
                }
            }
            .overlay {
                if store.outdated.isEmpty {
                    ContentUnavailableView(
                        "Everything is up to date", systemImage: "checkmark.circle",
                        description: lastChecked.map { Text($0) })
                }
            }
        }
    }

    private var subtitle: String? {
        store.pendingUpgradeCount > 0 ? "\(store.pendingUpgradeCount) available" : nil
    }

    private var lastChecked: String? {
        store.lastChecked.map { "Checked \($0.formatted(.relative(presentation: .named)))." }
    }
}

/// Packages with a newer release that a project's ketch.lock pins: shown so
/// the user knows, never upgraded from here.
private struct PinnedCard: View {
    let pinned: [Upgrade]

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.xs) {
            Text("Pinned · held by ketch.lock")
                .textStyle(Tokens.Typography.headline)
                .foregroundStyle(Tokens.Colors.Text.primary)
            ForEach(pinned) { pin in
                Text("\(pin.name) \(pin.from) → \(pin.to) · pinned in \(pin.heldBy ?? "ketch.lock")")
                    .textStyle(Tokens.Typography.callout)
                    .foregroundStyle(Tokens.Colors.Text.secondary)
            }
        }
        .glassCard(cornerRadius: Theme.Radius.card, padding: Tokens.Space.lg)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("updates-pinned")
    }
}
