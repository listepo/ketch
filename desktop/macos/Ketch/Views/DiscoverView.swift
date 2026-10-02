// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The registry: a hero for one package not installed yet, quick searches, and
// a shelf of app cards; typing in the search field turns the shelf into the
// results.

import SwiftUI

struct DiscoverView: View {
    @Environment(KetchStore.self) private var store
    @State private var query = ""

    /// Quick searches. The registry has no categories, so a chip is a search
    /// for a word most packages of that kind mention.
    private static let chips = ["Search", "Git", "JSON", "Python", "Shell"]

    var body: some View {
        NavigationStack {
            Page(title: "Discover") {
                SearchField(prompt: "Search the registry", text: $query)
            } content: {
                ScrollView {
                    VStack(alignment: .leading, spacing: Theme.Spacing.page) {
                        if isBrowsing, let hero {
                            HeroCard(package: hero, isInstalled: isInstalled(hero))
                        }
                        chips
                        SectionLabel(text: isBrowsing ? "In the registry" : "Results")
                        shelf
                    }
                    .padding(.horizontal, Theme.Spacing.window)
                    .padding(.vertical, Tokens.Space.sm)
                }
                .overlay {
                    if !isBrowsing && store.searchResults.isEmpty {
                        ContentUnavailableView.search(text: query)
                    }
                }
            }
            // Debounced: `task(id:)` cancels the previous search as the user types.
            .task(id: query) {
                try? await Task.sleep(for: .milliseconds(250))
                guard !Task.isCancelled else { return }
                await store.search(query)
            }
            .navigationDestination(for: String.self) { name in
                PackageDetailView(name: name)
            }
        }
    }

    private var isBrowsing: Bool { query.trimmingCharacters(in: .whitespaces).isEmpty }

    /// The first package the user does not have yet, else the first of all.
    private var hero: RegistryPackage? {
        store.searchResults.first { !isInstalled($0) } ?? store.searchResults.first
    }

    private var shelfPackages: [RegistryPackage] {
        guard isBrowsing, let hero else { return store.searchResults }
        return store.searchResults.filter { $0 != hero }
    }

    private func isInstalled(_ package: RegistryPackage) -> Bool {
        store.installed.contains { $0.name == package.name }
    }

    private var chips: some View {
        GlassEffectContainer(spacing: Tokens.Space.sm) {
            HStack(spacing: Tokens.Space.sm) {
                ForEach(Self.chips, id: \.self) { chip in
                    let isOn = query.caseInsensitiveCompare(chip) == .orderedSame
                    Button(chip) { query = isOn ? "" : chip.lowercased() }
                        .buttonStyle(.plain)
                        .textStyle(Tokens.Typography.callout)
                        .foregroundStyle(Tokens.Colors.Text.primary)
                        .padding(.horizontal, Tokens.Space.md)
                        .padding(.vertical, Tokens.Space.xs + Tokens.Space.xxs)
                        .glassSurface(
                            .capsule, level: .elevated, tint: isOn ? Tokens.Colors.Accent.subtle : nil,
                            interactive: true
                        )
                        .accessibilityAddTraits(isOn ? .isSelected : [])
                }
            }
        }
    }

    private var shelf: some View {
        GlassEffectContainer(spacing: Theme.Spacing.cards) {
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: Tokens.Size.menuBarWidth), spacing: Tokens.Space.sm)],
                spacing: Tokens.Space.sm
            ) {
                ForEach(shelfPackages) { package in
                    NavigationLink(value: package.name) {
                        PackageRow(name: package.name, subtitle: package.description ?? package.repo) {
                            if isInstalled(package) {
                                StatusBadge(tone: .installed, label: "Installed")
                            } else {
                                Button("Get") { Task { await store.install(package.name) } }
                                    .buttonStyle(.tint)
                                    .disabled(store.isRunning)
                                    .accessibilityLabel("Get \(package.name)")
                            }
                        }
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("discover-\(package.name)")
                }
            }
        }
    }
}

/// One package, large: its name and latest version, what it does, Get, and
/// what installing it does, as the terminal would show it.
private struct HeroCard: View {
    @Environment(KetchStore.self) private var store
    let package: RegistryPackage
    let isInstalled: Bool

    var body: some View {
        HStack(alignment: .center, spacing: Theme.Spacing.page) {
            VStack(alignment: .leading, spacing: Tokens.Space.sm) {
                Text(isInstalled ? "Installed" : "Not installed yet")
                    .textStyle(Tokens.Typography.overline)
                    .textCase(.uppercase)
                    .foregroundStyle(Tokens.Colors.Accent.ink)
                Text([package.name, package.latest].compactMap { $0 }.joined(separator: " "))
                    .textStyle(Tokens.Typography.display)
                    .foregroundStyle(Tokens.Colors.Text.primary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.6)
                if let description = package.description {
                    Text(description)
                        .textStyle(Tokens.Typography.body)
                        .foregroundStyle(Tokens.Colors.Text.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if !isInstalled {
                    Button("Get") { Task { await store.install(package.name) } }
                        .buttonStyle(.glassProminent)
                        .controlSize(.large)
                        .disabled(store.isRunning)
                        .accessibilityLabel("Get \(package.name)")
                        .padding(.top, Tokens.Space.xs)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Terminal(lines: lines)
                .frame(maxWidth: .infinity)
        }
        .glassCard(cornerRadius: Theme.Radius.panel, padding: Theme.Spacing.panelPadding)
    }

    private var lines: [String] {
        let bin = store.root.appending(path: "bin/\(package.name)").path.abbreviatingHome
        return [
            "$ ketch install \(package.name)",
            "resolve  \(package.repo)",
            "verify   checksum",
            "link     \(bin)",
        ]
    }
}

/// Monospaced lines on dark glass, in both appearances, like a terminal.
private struct Terminal: View {
    let lines: [String]

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.xs) {
            ForEach(lines, id: \.self) { line in
                Text(line).textStyle(Tokens.Typography.mono).lineLimit(1).truncationMode(.middle)
            }
        }
        .foregroundStyle(Tokens.Colors.Text.primary)
        .padding(.horizontal, Tokens.Space.lg)
        .padding(.vertical, Tokens.Space.md)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassSurface(.rect(cornerRadius: Theme.Radius.card), tint: Tokens.Colors.Preset.Tint.smoke)
        // A terminal is dark whatever the window is; the dark scheme also
        // picks the light ink that stays legible on it.
        .environment(\.colorScheme, .dark)
        .accessibilityElement(children: .combine)
    }
}
