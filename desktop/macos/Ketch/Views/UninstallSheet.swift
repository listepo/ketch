// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The uninstall confirmation, shared by the Installed list and the package
// detail: what goes, on glass, with Uninstall as the destructive action.

import SwiftUI

struct UninstallSheet: View {
    @Environment(KetchStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let package: InstalledPackage

    var body: some View {
        VStack(spacing: Tokens.Space.md) {
            AppIcon(name: package.name)
            Text("Uninstall \(package.name)?")
                .textStyle(Tokens.Typography.title)
                .foregroundStyle(Tokens.Colors.Text.primary)
                .multilineTextAlignment(.center)
            Text(
                "Removes \(package.name) \(package.version), its links in \(binDir) and its store directory. Its own configuration is kept."
            )
            .textStyle(Tokens.Typography.body)
            .foregroundStyle(Tokens.Colors.Text.secondary)
            .multilineTextAlignment(.center)
            .fixedSize(horizontal: false, vertical: true)
            GlassEffectContainer {
                HStack(spacing: Tokens.Space.sm) {
                    Button("Cancel", role: .cancel) { dismiss() }
                        .buttonStyle(.glass)
                        .keyboardShortcut(.cancelAction)
                    Button("Uninstall", role: .destructive) {
                        dismiss()
                        Task { await store.uninstall([package.name]) }
                    }
                    .destructiveProminent()
                    .disabled(store.isRunning)
                    .accessibilityIdentifier("uninstall-confirm")
                }
                .controlSize(.large)
            }
            .padding(.top, Tokens.Space.xs)
        }
        .frame(width: Tokens.Size.menuBarWidth)
        .padding(Tokens.Component.Sheet.padding)
    }

    private var binDir: String { store.root.appending(path: "bin").path.abbreviatingHome }
}
