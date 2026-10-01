// `ketch doctor` findings.

import SwiftUI

struct DoctorView: View {
    @Environment(KetchStore.self) private var store

    var body: some View {
        ScrollView {
            GlassEffectContainer(spacing: Theme.Spacing.cards) {
                LazyVStack(spacing: Theme.Spacing.cards) {
                    ForEach(store.findings) { finding in row(finding) }
                }
                .padding()
            }
        }
        .onBackdrop()
        .overlay {
            if store.findings.isEmpty {
                ContentUnavailableView("No checks run yet", systemImage: "stethoscope")
            }
        }
        .navigationTitle("Doctor")
        .toolbar {
            Button("Run Checks", systemImage: "play") { Task { await store.runDoctor() } }
        }
        .task { await store.runDoctor() }
    }

    private func row(_ finding: Finding) -> some View {
        HStack {
            Image(systemName: symbol(finding.severity))
                .foregroundStyle(color(finding.severity))
            Text(finding.message)
            Spacer()
            if let fix = finding.fix {
                // The core does not expose fix actions yet (R9 returns
                // findings only), so the fix is named, not offered.
                Text("Fix: \(fix)").font(.caption).foregroundStyle(.secondary)
            }
        }
        .glassCard()
    }

    private func symbol(_ severity: Finding.Severity) -> String {
        switch severity {
        case .ok: "checkmark.circle.fill"
        case .warning: "exclamationmark.triangle.fill"
        case .error: "xmark.octagon.fill"
        }
    }

    private func color(_ severity: Finding.Severity) -> Color {
        switch severity {
        case .ok: Theme.Palette.ok
        case .warning: Theme.Palette.warning
        case .error: Theme.Palette.error
        }
    }
}
