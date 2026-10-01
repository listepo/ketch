// The main window: a sidebar of sections, and the app-wide sheets and alerts
// (binary choice, busy lock, errors, upgrade-all confirmation) that any
// section — or the menu bar — can raise.

import SwiftUI

enum Section: String, Hashable, CaseIterable, Identifiable {
    case installed, discover, activity, doctor

    var id: Self { self }

    var title: String {
        switch self {
        case .installed: "Installed"
        case .discover: "Discover"
        case .activity: "Activity"
        case .doctor: "Doctor"
        }
    }

    var symbol: String {
        switch self {
        case .installed: "shippingbox"
        case .discover: "magnifyingglass"
        case .activity: "arrow.down.circle"
        case .doctor: "stethoscope"
        }
    }
}

struct ContentView: View {
    @Environment(KetchStore.self) private var store
    @Environment(\.appearsActive) private var appearsActive
    @State private var section: Section? = .installed

    var body: some View {
        @Bindable var store = store
        NavigationSplitView {
            List(Section.allCases, selection: $section) { item in
                Label(item.title, systemImage: item.symbol)
                    .badge(badge(for: item))
                    .accessibilityIdentifier("sidebar-\(item.rawValue)")
            }
            .navigationSplitViewColumnWidth(min: 170, ideal: 190)
            // macOS 26 draws the sidebar and toolbars on Liquid Glass itself;
            // extending the backdrop under the sidebar gives that glass colour to refract.
            .background(Backdrop())
        } detail: {
            switch section ?? .installed {
            case .installed: InstalledView()
            case .discover: DiscoverView()
            case .activity: ActivityView()
            case .doctor: DoctorView()
            }
        }
        // Installs made from the CLI show up when the user comes back.
        .onChange(of: appearsActive) { _, active in
            if active { Task { await store.refresh() } }
        }
        .task { await store.refresh() }
        .onChange(of: store.isRunning) { _, running in
            if running { section = .activity }
        }
        .sheet(item: $store.pendingChoice) { choice in
            BinaryChoiceSheet(choice: choice)
        }
        .alert(
            "ketch is busy",
            isPresented: Binding(get: { store.busy != nil }, set: { if !$0 { store.busy = nil } }),
            presenting: store.busy
        ) { busy in
            Button("Retry") { Task { await busy.retry() } }
            Button("Cancel", role: .cancel) {}
        } message: { busy in
            Text("ketch is running in another process (pid \(busy.pid)). Retry when it has finished.")
        }
        .alert(
            "Something went wrong",
            isPresented: Binding(get: { store.errorMessage != nil }, set: { if !$0 { store.errorMessage = nil } })
        ) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(store.errorMessage ?? "")
        }
        .confirmationDialog(
            "Upgrade \(store.pendingUpgradeCount) packages?",
            isPresented: $store.confirmingUpgradeAll
        ) {
            Button("Upgrade All") { Task { await store.upgrade() } }
        } message: {
            Text(store.outdated.map { "\($0.name) \($0.from) → \($0.to)" }.joined(separator: "\n"))
        }
    }

    private func badge(for item: Section) -> Int {
        item == .installed ? store.pendingUpgradeCount : 0
    }
}

/// The decider's question: which of a package's binaries to link.
struct BinaryChoiceSheet: View {
    @Environment(KetchStore.self) private var store
    let choice: BinaryChoice
    @State private var selected = 0

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.Spacing.page) {
            Text("Choose a binary for \(choice.package)")
                .font(.headline)
            Text("The release ships several executables. Pick the one to link onto PATH.")
                .foregroundStyle(.secondary)
            Picker("Binary", selection: $selected) {
                ForEach(choice.candidates.indices, id: \.self) { index in
                    Text(choice.candidates[index]).monospaced().tag(index)
                }
            }
            .pickerStyle(.radioGroup)
            .labelsHidden()
            .glassCard()
            GlassEffectContainer {
                HStack {
                    Spacer()
                    Button("Cancel", role: .cancel) { store.answer(nil) }
                        .buttonStyle(.glass)
                        .keyboardShortcut(.cancelAction)
                    Button("Link") { store.answer(selected) }
                        .buttonStyle(.glassProminent)
                        .keyboardShortcut(.defaultAction)
                }
            }
        }
        .padding(20)
        .frame(minWidth: 360)
        .interactiveDismissDisabled()
    }
}
