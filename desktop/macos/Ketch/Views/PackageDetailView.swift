// One package: description, versions, repository link and changelog.

import SwiftUI

struct PackageDetailView: View {
    @Environment(KetchStore.self) private var store
    let name: String
    @State private var changelog: AttributedString?
    @State private var loading = true

    private var installed: InstalledPackage? { store.installed.first { $0.name == name } }
    private var registry: RegistryPackage? { store.searchResults.first { $0.name == name } }
    private var latest: String? { store.outdatedVersion(of: name) ?? registry?.latest }
    private var repo: String? { installed?.repo ?? registry?.repo }

    var body: some View {
        ScrollView {
            GlassEffectContainer(spacing: Theme.Spacing.page) {
                VStack(alignment: .leading, spacing: Theme.Spacing.page) {
                    VStack(alignment: .leading, spacing: Theme.Spacing.section) {
                        Text(name).font(.largeTitle.bold())
                        if let description = installed?.description ?? registry?.description {
                            Text(description).foregroundStyle(.secondary)
                        }
                        Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 4) {
                            GridRow {
                                Text("Installed").foregroundStyle(.secondary)
                                Text(installed?.version ?? "—").monospacedDigit()
                            }
                            GridRow {
                                Text("Latest").foregroundStyle(.secondary)
                                Text(latest ?? "—").monospacedDigit()
                            }
                        }
                        HStack {
                            if let installed, let latest, latest != installed.version {
                                Button("Upgrade to \(latest)") { Task { await store.upgrade([name]) } }
                                    .buttonStyle(.glassProminent)
                                    .disabled(store.isRunning)
                            } else if installed == nil {
                                Button("Install") { Task { await store.install(name) } }
                                    .buttonStyle(.glassProminent)
                                    .disabled(store.isRunning)
                            }
                            if let repo, let url = URL(string: "https://github.com/\(repo)/releases") {
                                Link(destination: url) { Label("Releases", systemImage: "arrow.up.right") }
                                    .buttonStyle(.glass)
                            }
                        }
                    }
                    .glassCard(cornerRadius: Theme.Radius.panel)
                    VStack(alignment: .leading, spacing: Theme.Spacing.section) {
                        Text("Changelog").font(.title2.bold())
                        if loading {
                            ProgressView()
                        } else if let changelog {
                            Text(changelog).textSelection(.enabled)
                        } else {
                            Text("No changelog found.").foregroundStyle(.secondary)
                        }
                    }
                    .glassCard(cornerRadius: Theme.Radius.panel)
                }
                .padding()
            }
        }
        .onBackdrop()
        .navigationTitle(name)
        .task(id: name) {
            loading = true
            changelog = await store.changelog(for: name, from: installed?.version, to: latest)
            loading = false
        }
    }
}
