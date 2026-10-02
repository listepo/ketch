// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The running operation with per-package progress and Cancel, then the log
// grouped by day, newest first, filterable and exportable as plain text.

import AppKit
import SwiftUI
import UniformTypeIdentifiers

struct ActivityView: View {
    @Environment(KetchStore.self) private var store
    @State private var filter = ""

    var body: some View {
        Page(title: "Activity") {
            SearchField(prompt: "Filter history", text: $filter)
            Button("Export log") { export() }
                .buttonStyle(.glass)
                .disabled(store.log.isEmpty)
        } content: {
            PageScroll {
                if let activity = store.activity {
                    RunningCard(activity: activity)
                }
                LogCard(days: days, isFiltered: !needle.isEmpty)
            }
        }
    }

    private var needle: String { filter.trimmingCharacters(in: .whitespaces) }

    /// The log, newest first, in one group per calendar day.
    private var days: [(day: Date, entries: [LogEntry])] {
        let calendar = Calendar.current
        let entries = store.log.reversed().filter {
            needle.isEmpty || $0.message.localizedCaseInsensitiveContains(needle)
        }
        var days: [(day: Date, entries: [LogEntry])] = []
        for entry in entries {
            let day = calendar.startOfDay(for: entry.date)
            if days.last?.day == day {
                days[days.count - 1].entries.append(entry)
            } else {
                days.append((day, [entry]))
            }
        }
        return days
    }

    private func export() {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "ketch-activity.log"
        panel.allowedContentTypes = [.log, .plainText]
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let text = store.log.map { entry in
            "\(entry.date.formatted(.iso8601)) \(entry.level.word) \(entry.message)"
        }
        do {
            try (text.joined(separator: "\n") + "\n").write(to: url, atomically: true, encoding: .utf8)
        } catch {
            store.errorMessage = error.localizedDescription
        }
    }
}

/// The operation in progress: one bar per package, its status, and Cancel.
private struct RunningCard: View {
    @Environment(KetchStore.self) private var store
    let activity: Activity

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.sm) {
            HStack {
                Text(activity.title).textStyle(Tokens.Typography.headline)
                Spacer()
                Button(activity.isCancelling ? "Cancelling…" : "Cancel", role: .cancel) {
                    store.cancel()
                }
                .buttonStyle(.glass)
                .disabled(activity.isCancelling)
                .accessibilityIdentifier("activity-cancel")
            }
            ForEach(activity.packages.keys.sorted(), id: \.self) { name in
                if let progress = activity.packages[name] {
                    ProgressView(value: progress.fraction) {
                        Text(name).textStyle(Tokens.Typography.callout)
                    } currentValueLabel: {
                        Text(progress.stage.rawValue.capitalized)
                            .textStyle(Tokens.Typography.caption)
                            .foregroundStyle(Tokens.Colors.Text.secondary)
                    }
                }
            }
            if let status = activity.status {
                Text(status).textStyle(Tokens.Typography.callout).foregroundStyle(Tokens.Colors.Text.secondary)
            }
        }
        .foregroundStyle(Tokens.Colors.Text.primary)
        .glassCard(cornerRadius: Theme.Radius.panel, padding: Tokens.Space.lg)
    }
}

/// The log as the design's history card: a day label, then time, glyph and
/// message per line.
private struct LogCard: View {
    let days: [(day: Date, entries: [LogEntry])]
    let isFiltered: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.sm) {
            if days.isEmpty {
                Text(isFiltered ? "No entries match." : "Nothing has happened yet.")
                    .textStyle(Tokens.Typography.body)
                    .foregroundStyle(Tokens.Colors.Text.secondary)
            }
            ForEach(days, id: \.day) { group in
                SectionLabel(text: label(for: group.day))
                    .padding(.top, group.day == days.first?.day ? 0 : Tokens.Space.xs)
                ForEach(group.entries) { entry in
                    row(entry)
                }
            }
        }
        .glassCard(cornerRadius: Theme.Radius.panel, padding: Tokens.Space.lg)
    }

    private func row(_ entry: LogEntry) -> some View {
        HStack(spacing: Tokens.Space.md) {
            Text(entry.date, format: .dateTime.hour(.twoDigits(amPM: .omitted)).minute(.twoDigits))
                .textStyle(Tokens.Typography.mono)
                .foregroundStyle(Tokens.Colors.Text.secondary)
                .help(entry.date.formatted(date: .abbreviated, time: .standard))
            StatusGlyph(tone: entry.level.tone, symbol: entry.level.symbol)
            Text(entry.message)
                .textStyle(Tokens.Typography.body)
                .foregroundStyle(Tokens.Colors.Text.primary)
                .textSelection(.enabled)
            Spacer(minLength: 0)
        }
        .accessibilityElement(children: .combine)
    }

    private func label(for day: Date) -> String {
        let calendar = Calendar.current
        if calendar.isDateInToday(day) { return "Today" }
        if calendar.isDateInYesterday(day) { return "Yesterday" }
        return day.formatted(date: .abbreviated, time: .omitted)
    }
}

extension LogEntry.Level {
    fileprivate var tone: StatusTone {
        switch self {
        case .info: .busy
        case .success: .installed
        case .warning: .warning
        case .error: .error
        }
    }

    fileprivate var symbol: String {
        switch self {
        case .info: "arrow.right"
        case .success: "checkmark"
        case .warning: "exclamationmark"
        case .error: "xmark"
        }
    }

    /// The level in an exported log line.
    fileprivate var word: String {
        switch self {
        case .info: "INFO"
        case .success: "DONE"
        case .warning: "WARN"
        case .error: "ERROR"
        }
    }
}
