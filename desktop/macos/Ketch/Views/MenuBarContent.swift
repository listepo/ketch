// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

// The menu-bar extra: the update count with Update all, a row per pending
// update, the running operation, then Open, Check for updates, Settings and
// Quit as menu rows — a glass panel (`.menuBarExtraStyle(.window)`) rather
// than a plain menu, so it can show progress and match the main window.

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

    /// More rows than this would push the menu rows off a small screen; the
    /// window lists them all.
    private static let maxRows = 5

    var body: some View {
        GlassEffectContainer(spacing: Theme.Spacing.cards) {
            VStack(alignment: .leading, spacing: Theme.Spacing.cards) {
                header
                ForEach(store.updates.prefix(Self.maxRows)) { update in
                    PackageRow(name: update.name, subtitle: "\(update.from) → \(update.to)") {
                        Button("Update") { Task { await store.upgrade([update.name]) } }
                            .buttonStyle(.tint)
                            .disabled(store.isRunning)
                            .accessibilityLabel("Update \(update.name)")
                    }
                }
                if let activity = store.activity {
                    progress(activity)
                }
                Divider()
                VStack(spacing: 0) {
                    MenuRow(title: "Open Ketch", shortcut: "⌘O") { openMain() }
                        .keyboardShortcut("o")
                    MenuRow(title: "Check for updates", shortcut: "⌘R") { Task { await store.refresh() } }
                        .keyboardShortcut("r")
                        .disabled(store.isRunning)
                    SettingsLink { MenuRowLabel(title: "Settings…", shortcut: "⌘,") }
                        .buttonStyle(.plain)
                        .keyboardShortcut(",")
                }
                Divider()
                MenuRow(title: "Quit Ketch", shortcut: "⌘Q") { NSApplication.shared.terminate(nil) }
                    .keyboardShortcut("q")
            }
            .padding(Theme.Spacing.cardPadding)
        }
        .frame(width: Tokens.Component.MenuBarExtra.width)
        .background(Backdrop())
    }

    private var header: some View {
        HStack(spacing: Tokens.Space.sm) {
            AppIcon(name: "ketch")
            VStack(alignment: .leading, spacing: Tokens.Space.xxs) {
                Text("Ketch").textStyle(Tokens.Typography.headline).foregroundStyle(Tokens.Colors.Text.primary)
                Text(summary).textStyle(Tokens.Typography.caption).foregroundStyle(Tokens.Colors.Text.secondary)
            }
            Spacer(minLength: Tokens.Space.sm)
            // The confirmation is the main window's; the panel only asks for it.
            Button("Update all") {
                openMain()
                store.confirmingUpgradeAll = true
            }
            .buttonStyle(.glassProminent)
            .controlSize(.small)
            .disabled(store.pendingUpgradeCount == 0 || store.isRunning)
        }
        .padding(.horizontal, Tokens.Space.xs)
    }

    private func progress(_ activity: Activity) -> some View {
        HStack(spacing: Tokens.Space.sm) {
            Text(activity.packages.keys.sorted().first ?? activity.title)
                .textStyle(Tokens.Typography.caption)
                .foregroundStyle(Tokens.Colors.Text.primary)
                .lineLimit(1)
            ProgressView(value: activity.fraction)
                .accessibilityLabel(activity.title)
            Text(activity.fraction, format: .percent.precision(.fractionLength(0)))
                .textStyle(Tokens.Typography.caption)
                .foregroundStyle(Tokens.Colors.Text.secondary)
                .monospacedDigit()
            Button("Cancel", systemImage: "xmark.circle.fill") { store.cancel() }
                .labelStyle(.iconOnly)
                .buttonStyle(.plain)
                .foregroundStyle(Tokens.Colors.Text.secondary)
                .disabled(activity.isCancelling)
        }
        .padding(.horizontal, Tokens.Space.md)
        .frame(height: Tokens.Size.controlHeight + Tokens.Space.xs)
        .glassSurface(.capsule, level: .elevated)
    }

    private var summary: String {
        let updates = store.pendingUpgradeCount == 1 ? "1 update" : "\(store.pendingUpgradeCount) updates"
        return "\(updates) · \(store.installed.count) installed"
    }

    private func openMain() {
        openWindow(id: WindowID.main)
        NSApplication.shared.activate()
    }
}

/// A menu item inside the panel: a plain row that highlights under the
/// pointer, with its shortcut on the right.
private struct MenuRow: View {
    let title: String
    let shortcut: String
    let action: () -> Void

    var body: some View {
        Button(action: action) { MenuRowLabel(title: title, shortcut: shortcut) }
            .buttonStyle(.plain)
    }
}

private struct MenuRowLabel: View {
    let title: String
    let shortcut: String
    @State private var hovering = false
    @Environment(\.isEnabled) private var isEnabled

    var body: some View {
        HStack {
            Text(title)
                .foregroundStyle(isEnabled ? Tokens.Colors.Text.primary : Tokens.Colors.Text.tertiary)
            Spacer()
            Text(shortcut).foregroundStyle(Tokens.Colors.Text.secondary).accessibilityHidden(true)
        }
        .textStyle(Tokens.Typography.body)
        .padding(.horizontal, Tokens.Space.sm)
        .frame(height: Tokens.Size.controlHeight)
        .contentShape(.rect)
        .background {
            if hovering && isEnabled {
                RoundedRectangle(cornerRadius: Theme.Radius.control).fill(Tokens.Colors.Fill.hover)
            }
        }
        .onHover { hovering = $0 }
    }
}
