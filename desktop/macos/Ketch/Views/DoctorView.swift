// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

// `ketch doctor` findings: a tally of passed, warning and failed checks, then
// one row per check.

import SwiftUI

struct DoctorView: View {
    @Environment(KetchStore.self) private var store

    var body: some View {
        Page(title: "Doctor") {
            Button("Run again") { Task { await store.runDoctor() } }
                .buttonStyle(.glass)
        } content: {
            PageScroll {
                if !store.findings.isEmpty {
                    HStack(spacing: Tokens.Space.sm) {
                        Tally(count: count(.ok), label: "passed", tone: .installed)
                        Tally(
                            count: count(.warning), label: count(.warning) == 1 ? "warning" : "warnings", tone: .warning
                        )
                        Tally(count: count(.error), label: "failed", tone: .error)
                    }
                    .padding(.bottom, Tokens.Space.xs)
                }
                ForEach(store.findings) { finding in row(finding) }
            }
            .overlay {
                if store.findings.isEmpty {
                    ContentUnavailableView("No checks run yet", systemImage: "stethoscope")
                }
            }
        }
        .task { await store.runDoctor() }
    }

    private func count(_ severity: Finding.Severity) -> Int {
        store.findings.count { $0.severity == severity }
    }

    private func row(_ finding: Finding) -> some View {
        HStack(spacing: Tokens.Space.md) {
            StatusGlyph(tone: tone(finding.severity), symbol: symbol(finding.severity))
            VStack(alignment: .leading, spacing: Tokens.Space.xxs) {
                Text(finding.message)
                    .textStyle(Tokens.Typography.headline)
                    .foregroundStyle(Tokens.Colors.Text.primary)
                if let fix = finding.fix {
                    // The core does not expose fix actions yet (R9 returns
                    // findings only), so the fix is named, not offered.
                    Text("Fix: \(fix)")
                        .textStyle(Tokens.Typography.callout)
                        .foregroundStyle(Tokens.Colors.Text.secondary)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, Tokens.Space.lg)
        .padding(.vertical, Tokens.Space.md)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassSurface(.rect(cornerRadius: Theme.Radius.card))
        .liftShadow()
        .accessibilityElement(children: .combine)
    }

    private func symbol(_ severity: Finding.Severity) -> String {
        switch severity {
        case .ok: "checkmark"
        case .warning: "exclamationmark"
        case .error: "xmark"
        }
    }

    private func tone(_ severity: Finding.Severity) -> StatusTone {
        switch severity {
        case .ok: .installed
        case .warning: .warning
        case .error: .error
        }
    }
}

/// One figure of the summary: a count in its status colour and what it counts.
private struct Tally: View {
    let count: Int
    let label: String
    let tone: StatusTone

    var body: some View {
        HStack(spacing: Tokens.Space.xs) {
            Text(count, format: .number).textStyle(Tokens.Typography.headline).foregroundStyle(tone.color)
            Text(label).textStyle(Tokens.Typography.callout).foregroundStyle(Tokens.Colors.Text.secondary)
        }
        .padding(.horizontal, Tokens.Space.md)
        .frame(height: Tokens.Size.controlHeight + Tokens.Space.xs)
        .glassSurface(.capsule, level: .elevated)
        .accessibilityElement(children: .combine)
    }
}
