// The main window: a glass sidebar of sections with their counts, Settings and
// the ketch root at its foot, and the app-wide sheets and alerts (binary
// choice, errors, upgrade-all confirmation) that any section — or the menu
// bar — can raise. The running operation stays in sight as a bar along the
// bottom of every section but Activity, which shows it in full.

import SwiftUI

enum Section: String, Hashable, CaseIterable, Identifiable {
    case installed, discover, updates, activity, doctor, settings

    var id: Self { self }

    /// The sections listed at the top of the sidebar; Settings sits at its foot.
    static let listed: [Section] = [.installed, .discover, .updates, .activity, .doctor]

    var title: String {
        switch self {
        case .installed: "Installed"
        case .discover: "Discover"
        case .updates: "Updates"
        case .activity: "Activity"
        case .doctor: "Doctor"
        case .settings: "Settings"
        }
    }

    var symbol: String {
        switch self {
        case .installed: "shippingbox"
        case .discover: "safari"
        case .updates: "arrow.down.to.line"
        case .activity: "clock"
        case .doctor: "waveform.path.ecg"
        case .settings: "gearshape"
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
            Sidebar(section: $section)
                .navigationSplitViewColumnWidth(Tokens.Component.Sidebar.width)
        } detail: {
            detail
                .safeAreaInset(edge: .bottom, spacing: 0) {
                    if let activity = store.activity, section != .activity {
                        ActivityBar(activity: activity)
                    }
                }
        }
        // Installs made from the CLI show up when the user comes back.
        .onChange(of: appearsActive) { _, active in
            if active { Task { await store.refresh() } }
        }
        .onOpenURL { url in Task { await store.open(link: url) } }
        .onChange(of: store.linkedPackage) { _, name in
            if name != nil { section = .discover }
        }
        .task {
            await store.refresh()
            // The sidebar counts Doctor's problems before Doctor is opened.
            await store.runDoctor()
        }
        .sheet(item: $store.pendingChoice) { choice in
            BinaryChoiceSheet(choice: choice)
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
            "Update \(store.pendingUpgradeCount) packages?",
            isPresented: $store.confirmingUpgradeAll
        ) {
            Button("Update All") { Task { await store.upgrade() } }
        } message: {
            Text(store.updates.map { "\($0.name) \($0.from) → \($0.to)" }.joined(separator: "\n"))
        }
    }

    @ViewBuilder
    private var detail: some View {
        switch section ?? .installed {
        case .installed: InstalledView()
        case .discover: DiscoverView()
        case .updates: UpdatesView()
        case .activity: ActivityView()
        case .doctor: DoctorView()
        case .settings:
            Page(title: "Settings") {
                SettingsView()
                    .padding(.horizontal, Theme.Spacing.window)
                    .padding(.bottom, Theme.Spacing.window)
            }
        }
    }
}

/// The sections with their counts, then Settings and the root it manages.
private struct Sidebar: View {
    @Environment(KetchStore.self) private var store
    @Binding var section: Section?

    var body: some View {
        // macOS 26 draws the sidebar on Liquid Glass itself, with the
        // selection as a raised glass capsule; extending the backdrop under it
        // gives that glass colour to refract.
        List(Section.listed, selection: $section) { item in
            Label(item.title, systemImage: item.symbol)
                .textStyle(Tokens.Typography.body)
                .badge(count(for: item))
                .accessibilityIdentifier("sidebar-\(item.rawValue)")
        }
        .safeAreaInset(edge: .bottom, spacing: 0) { foot }
        .background(Backdrop())
    }

    private var foot: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.sm) {
            Button {
                section = .settings
            } label: {
                Label(Section.settings.title, systemImage: Section.settings.symbol)
                    .textStyle(Tokens.Typography.body)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, Tokens.Space.sm)
                    .frame(height: Tokens.Size.controlHeight + Tokens.Space.xs)
                    .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .background {
                if section == .settings {
                    RoundedRectangle(cornerRadius: Theme.Radius.control).fill(Tokens.Colors.Fill.hover)
                }
            }
            .accessibilityIdentifier("sidebar-settings")
            .accessibilityAddTraits(section == .settings ? .isSelected : [])

            VStack(alignment: .leading, spacing: Tokens.Space.xxs) {
                Text(store.root.path.abbreviatingHome)
                    .textStyle(Tokens.Typography.headline)
                    .foregroundStyle(Tokens.Colors.Text.primary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text(packageCount)
                    .textStyle(Tokens.Typography.caption)
                    .foregroundStyle(Tokens.Colors.Text.secondary)
            }
            .padding(.horizontal, Tokens.Space.sm)
            .accessibilityElement(children: .combine)
        }
        .padding(.horizontal, Tokens.Space.sm)
        .padding(.bottom, Tokens.Space.md)
    }

    private var packageCount: String {
        store.installed.count == 1 ? "1 package" : "\(store.installed.count) packages"
    }

    private func count(for item: Section) -> Int {
        switch item {
        case .installed: store.installed.count
        case .updates: store.pendingUpgradeCount
        case .doctor: store.problemCount
        case .discover, .activity, .settings: 0
        }
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
                .textStyle(Tokens.Typography.title2)
            Text("The release ships several executables. Pick the one to link onto PATH.")
                .foregroundStyle(Tokens.Colors.Text.secondary)
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
        .padding(Tokens.Component.Sheet.padding)
        .frame(minWidth: Tokens.Size.menuBarWidth)
        .interactiveDismissDisabled()
    }
}
