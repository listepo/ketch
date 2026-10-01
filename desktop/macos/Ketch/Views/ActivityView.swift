// The running operation, per-package progress, and the log.

import SwiftUI

struct ActivityView: View {
    @Environment(KetchStore.self) private var store

    var body: some View {
        GlassEffectContainer(spacing: Theme.Spacing.section) {
            VStack(alignment: .leading, spacing: Theme.Spacing.section) {
                Group {
                    if let activity = store.activity {
                        VStack(alignment: .leading, spacing: Theme.Spacing.cards) {
                            HStack {
                                Text(activity.title).font(.headline)
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
                                        Text(name)
                                    } currentValueLabel: {
                                        Text(progress.stage.rawValue.capitalized)
                                    }
                                }
                            }
                            if let status = activity.status {
                                Text(status).foregroundStyle(.secondary)
                            }
                        }
                    } else {
                        Label("Nothing running.", systemImage: "checkmark.circle").foregroundStyle(.secondary)
                    }
                }
                .glassCard(cornerRadius: Theme.Radius.panel)
                VStack(alignment: .leading, spacing: 8) {
                    Text("Log").font(.headline)
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 4) {
                            ForEach(store.log.reversed()) { entry in
                                HStack(alignment: .firstTextBaseline) {
                                    Text(entry.date, format: .dateTime.hour().minute().second())
                                        .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                                    Text(entry.message).foregroundStyle(color(entry.level))
                                }
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                .frame(maxHeight: .infinity, alignment: .top)
                .glassCard(cornerRadius: Theme.Radius.panel)
            }
            .padding()
        }
        .onBackdrop()
        .navigationTitle("Activity")
    }

    private func color(_ level: LogEntry.Level) -> Color {
        switch level {
        case .info: .primary
        case .warning: Theme.Palette.warning
        case .error: Theme.Palette.error
        }
    }
}
