// Installed packages as glass cards: version, source and an update badge,
// with upgrade, uninstall and reveal-in-Finder actions.

import SwiftUI

struct InstalledView: View {
    @Environment(KetchStore.self) private var store
    @State private var pendingUninstall: InstalledPackage?

    var body: some View {
        @Bindable var store = store
        NavigationStack {
            ScrollView {
                GlassEffectContainer(spacing: Theme.Spacing.cards) {
                    LazyVStack(spacing: Theme.Spacing.cards) {
                        ForEach(store.installed) { package in
                            NavigationLink(value: package.name) {
                                InstalledRow(package: package, update: store.outdatedVersion(of: package.name))
                            }
                            .buttonStyle(.plain)
                            .contextMenu { menu(for: package) }
                        }
                    }
                    .padding()
                }
            }
            .overlay {
                if store.installed.isEmpty {
                    ContentUnavailableView(
                        "Nothing installed", systemImage: "shippingbox",
                        description: Text("Find packages in Discover."))
                }
            }
            .onBackdrop()
            .navigationTitle("Installed")
            .navigationDestination(for: String.self) { name in
                PackageDetailView(name: name)
            }
            .toolbar {
                ToolbarItemGroup {
                    Button("Refresh", systemImage: "arrow.clockwise") { Task { await store.refresh() } }
                    Button("Upgrade All", systemImage: "arrow.up.circle") { store.confirmingUpgradeAll = true }
                        .disabled(store.pendingUpgradeCount == 0 || store.isRunning)
                }
            }
            .confirmationDialog(
                "Uninstall \(pendingUninstall?.name ?? "")?",
                isPresented: Binding(get: { pendingUninstall != nil }, set: { if !$0 { pendingUninstall = nil } }),
                presenting: pendingUninstall
            ) { package in
                Button("Uninstall", role: .destructive) { Task { await store.uninstall([package.name]) } }
            } message: { package in
                Text("This removes \(package.name) \(package.version) and its links.")
            }
        }
    }

    @ViewBuilder
    private func menu(for package: InstalledPackage) -> some View {
        if store.outdatedVersion(of: package.name) != nil {
            Button("Upgrade") { Task { await store.upgrade([package.name]) } }
        }
        Button("Reveal in Finder") {
            NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: package.path)])
        }
        Divider()
        Button("Uninstall…", role: .destructive) { pendingUninstall = package }
    }
}

private struct InstalledRow: View {
    let package: InstalledPackage
    let update: String?

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "shippingbox.fill")
                .font(.title2)
                .foregroundStyle(.tint)
            VStack(alignment: .leading, spacing: 2) {
                Text(package.name).font(.headline)
                if let description = package.description {
                    Text(description).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            Spacer()
            if let update {
                Text("→ \(update)")
                    .font(.caption.monospacedDigit().bold())
                    .padding(.horizontal, 8).padding(.vertical, 3)
                    .glassEffect(.regular.tint(Theme.Palette.updateBadge), in: .capsule)
                    .accessibilityLabel("Update to \(update) available")
            }
            Text(package.version).monospacedDigit().foregroundStyle(.secondary)
            Text(package.source).font(.caption).foregroundStyle(.tertiary)
        }
        .contentShape(.rect)
        .glassCard(interactive: true)
    }
}
