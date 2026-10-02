// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// One package: a large header with its actions (update or install, the
// repository, uninstall), Overview / Changelog / Files tabs, and the facts
// ketch knows about it beside them.

import SwiftUI

struct PackageDetailView: View {
    @Environment(KetchStore.self) private var store
    let name: String
    @State private var tab = DetailTab.changelog
    @State private var changelog: AttributedString?
    @State private var loading = true
    @State private var pendingUninstall: InstalledPackage?

    enum DetailTab: String, CaseIterable, Identifiable {
        case overview, changelog, files
        var id: Self { self }
        var title: String { rawValue.capitalized }
    }

    private var installed: InstalledPackage? { store.installed.first { $0.name == name } }
    private var registry: RegistryPackage? { store.searchResults.first { $0.name == name } }
    private var update: String? { store.outdatedVersion(of: name) }
    private var latest: String? { update ?? registry?.latest }
    private var repo: String? { installed?.repo ?? registry?.repo }

    var body: some View {
        ScrollView {
            GlassEffectContainer(spacing: Theme.Spacing.page) {
                VStack(alignment: .leading, spacing: Theme.Spacing.page) {
                    header
                    Picker("Section", selection: $tab) {
                        ForEach(DetailTab.allCases) { Text($0.title).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .fixedSize()
                    HStack(alignment: .top, spacing: Theme.Spacing.section) {
                        tabContent
                            .glassCard(cornerRadius: Theme.Radius.panel, padding: Tokens.Space.lg)
                        details
                            .glassCard(cornerRadius: Theme.Radius.panel, padding: Tokens.Space.lg)
                            .frame(width: Tokens.Size.sidebarWidth + Tokens.Space.huge)
                    }
                }
                .padding(.horizontal, Theme.Spacing.window)
                .padding(.vertical, Theme.Spacing.section)
            }
        }
        .onBackdrop()
        .navigationTitle(name)
        .sheet(item: $pendingUninstall) { package in
            UninstallSheet(package: package)
        }
        .task(id: name) {
            loading = true
            changelog = await store.changelog(for: name, from: installed?.version, to: latest)
            loading = false
        }
    }

    private var header: some View {
        HStack(alignment: .top, spacing: Theme.Spacing.page) {
            AppIcon(name: name, size: .hero)
            VStack(alignment: .leading, spacing: Tokens.Space.sm) {
                Text(name)
                    .textStyle(Tokens.Typography.display)
                    .foregroundStyle(Tokens.Colors.Text.primary)
                Text(summary)
                    .textStyle(Tokens.Typography.callout)
                    .foregroundStyle(Tokens.Colors.Text.secondary)
                GlassEffectContainer(spacing: Tokens.Space.sm) {
                    HStack(spacing: Tokens.Space.sm) {
                        if installed != nil, let update {
                            Button("Update to \(update)") { Task { await store.upgrade([name]) } }
                                .buttonStyle(.glassProminent)
                                .disabled(store.isRunning)
                        } else if installed == nil {
                            Button("Install") { Task { await store.install(name) } }
                                .buttonStyle(.glassProminent)
                                .disabled(store.isRunning)
                        }
                        if let repo, let url = URL(string: "https://github.com/\(repo)") {
                            Link("Open on GitHub", destination: url)
                                .buttonStyle(.glass)
                        }
                        if let installed {
                            Button("Uninstall") { pendingUninstall = installed }
                                .destructiveProminent()
                                .disabled(store.isRunning)
                        }
                    }
                    .controlSize(.large)
                }
            }
        }
    }

    /// `owner/repo · installed 1.0 · 1.1 available`, with whatever is known.
    private var summary: String {
        var parts: [String] = []
        if let repo { parts.append(repo) }
        if let installed { parts.append("installed \(installed.version)") }
        if let update {
            parts.append("\(update) available")
        } else if installed == nil, let latest {
            parts.append("latest \(latest)")
        }
        return parts.joined(separator: " · ")
    }

    @ViewBuilder
    private var tabContent: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.sm) {
            switch tab {
            case .overview:
                Text(installed?.description ?? registry?.description ?? "No description.")
                    .textStyle(Tokens.Typography.body)
            case .changelog:
                if loading {
                    ProgressView().controlSize(.small)
                } else if let changelog {
                    Text(changelog).textStyle(Tokens.Typography.body).textSelection(.enabled)
                } else {
                    Text("No changelog found.").foregroundStyle(Tokens.Colors.Text.secondary)
                }
            case .files:
                if let installed {
                    Text(installed.path.abbreviatingHome)
                        .textStyle(Tokens.Typography.mono)
                        .textSelection(.enabled)
                    Button("Reveal in Finder") {
                        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: installed.path)])
                    }
                    .buttonStyle(.glass)
                } else {
                    Text("Not installed.").foregroundStyle(Tokens.Colors.Text.secondary)
                }
            }
        }
        .foregroundStyle(Tokens.Colors.Text.primary)
    }

    private var details: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.sm) {
            Text("Details").textStyle(Tokens.Typography.headline).foregroundStyle(Tokens.Colors.Text.primary)
            Grid(alignment: .leading, horizontalSpacing: Tokens.Space.md, verticalSpacing: Tokens.Space.sm) {
                fact("Installed", installed?.version)
                fact("Latest", latest)
                fact("Source", installed?.source ?? (registry == nil ? nil : "github"))
                fact("Repository", repo)
                fact("Linked to", installed.map { _ in store.root.appending(path: "bin").path.abbreviatingHome })
            }
        }
    }

    private func fact(_ label: String, _ value: String?) -> some View {
        GridRow {
            Text(label).textStyle(Tokens.Typography.callout).foregroundStyle(Tokens.Colors.Text.secondary)
            Text(value ?? "—")
                .textStyle(Tokens.Typography.mono)
                .foregroundStyle(Tokens.Colors.Text.primary)
                .lineLimit(1)
                .truncationMode(.middle)
                .textSelection(.enabled)
        }
    }
}
