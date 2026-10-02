// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

// Installed packages as glass rows: icon, repository, version and a state
// badge, filtered by a search field, with Upgrade all in the header and
// upgrade, uninstall and reveal-in-Finder on each row's context menu.

import SwiftUI

struct InstalledView: View {
    @Environment(KetchStore.self) private var store
    @State private var query = ""
    @State private var pendingUninstall: InstalledPackage?

    var body: some View {
        NavigationStack {
            Page(title: "Installed") {
                SearchField(prompt: "Search installed", text: $query)
                Button(upgradeAllTitle) { store.confirmingUpgradeAll = true }
                    .buttonStyle(.glassProminent)
                    .disabled(store.pendingUpgradeCount == 0 || store.isRunning)
            } content: {
                PageScroll {
                    ForEach(packages) { package in
                        NavigationLink(value: package.name) { row(package) }
                            .buttonStyle(.plain)
                            .accessibilityIdentifier("installed-\(package.name)")
                            .contextMenu { menu(for: package) }
                    }
                }
                .overlay {
                    if store.installed.isEmpty {
                        ContentUnavailableView(
                            "Nothing installed", systemImage: "shippingbox",
                            description: Text("Find packages in Discover."))
                    } else if packages.isEmpty {
                        ContentUnavailableView.search(text: query)
                    }
                }
            }
            .navigationDestination(for: String.self) { name in
                PackageDetailView(name: name)
            }
            .sheet(item: $pendingUninstall) { package in
                UninstallSheet(package: package)
            }
        }
    }

    private var upgradeAllTitle: String {
        store.pendingUpgradeCount > 0 ? "Upgrade all \(store.pendingUpgradeCount)" : "Upgrade all"
    }

    private var packages: [InstalledPackage] {
        let needle = query.trimmingCharacters(in: .whitespaces)
        guard !needle.isEmpty else { return store.installed }
        return store.installed.filter { package in
            [package.name, package.repo, package.description].contains {
                $0?.localizedCaseInsensitiveContains(needle) ?? false
            }
        }
    }

    private func row(_ package: InstalledPackage) -> some View {
        PackageRow(
            name: package.name, subtitle: package.repo ?? package.description ?? package.source,
            version: package.version
        ) {
            if let update = store.outdatedVersion(of: package.name) {
                StatusBadge(tone: .update, label: "\(update) available")
                    .accessibilityLabel("Update to \(update) available")
            } else if let pin = store.pinned.first(where: { $0.name == package.name }) {
                StatusBadge(tone: .busy, label: "Pinned")
                    .help("Held at \(pin.from) by \(pin.heldBy ?? "ketch.lock")")
            } else {
                StatusBadge(tone: .installed, label: "Up to date")
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
