// Registry search and install.

import SwiftUI

struct DiscoverView: View {
    @Environment(KetchStore.self) private var store
    @State private var query = ""

    var body: some View {
        NavigationStack {
            ScrollView {
                GlassEffectContainer(spacing: Theme.Spacing.cards) {
                    LazyVStack(spacing: Theme.Spacing.cards) {
                        ForEach(store.searchResults) { package in
                            NavigationLink(value: package.name) { row(package) }
                                .buttonStyle(.plain)
                        }
                    }
                    .padding()
                }
            }
            .onBackdrop()
            .searchable(text: $query, prompt: "Search the registry")
            // Debounced: `task(id:)` cancels the previous search as the user types.
            .task(id: query) {
                try? await Task.sleep(for: .milliseconds(250))
                guard !Task.isCancelled else { return }
                await store.search(query)
            }
            .navigationTitle("Discover")
            .navigationDestination(for: String.self) { name in
                PackageDetailView(name: name)
            }
        }
    }

    private func row(_ package: RegistryPackage) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(package.name).font(.headline)
                Text(package.description ?? package.repo)
                    .font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer()
            if let latest = package.latest {
                Text(latest).monospacedDigit().foregroundStyle(.secondary)
            }
            if store.installed.contains(where: { $0.name == package.name }) {
                Label("Installed", systemImage: "checkmark.circle.fill")
                    .font(.caption).foregroundStyle(.secondary)
            } else {
                Button("Install") { Task { await store.install(package.name) } }
                    .buttonStyle(.glassProminent)
                    .disabled(store.isRunning)
            }
        }
        .contentShape(.rect)
        .glassCard(interactive: true)
    }
}
