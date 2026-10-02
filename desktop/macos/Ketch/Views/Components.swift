// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The pieces every screen is built from, one per component in the design's
// Components page: the page header, the package row, app icon, badge, tint
// button, search field, busy banner and the running-operation bar. Kept apart
// from the screens so a change to a component lands everywhere it is used.

import AppKit
import SwiftUI

/// A screen in the detail column: the large title with its actions, the busy
/// banner when the lock is held, then the content.
struct Page<Actions: View, Content: View>: View {
    let title: String
    var subtitle: String?
    @ViewBuilder var actions: Actions
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.Spacing.section) {
            HStack(alignment: .firstTextBaseline, spacing: Theme.Spacing.section) {
                Text(title)
                    .textStyle(Tokens.Typography.title)
                    .foregroundStyle(Tokens.Colors.Text.primary)
                    .accessibilityAddTraits(.isHeader)
                    .accessibilityIdentifier("page-\(title.lowercased())")
                if let subtitle {
                    Text(subtitle).textStyle(Tokens.Typography.callout).foregroundStyle(Tokens.Colors.Text.secondary)
                }
                Spacer(minLength: Theme.Spacing.section)
                HStack(spacing: Tokens.Space.sm) { actions }
                    // The title gives way first; an action cut to "Upgr…" is no action.
                    .fixedSize()
                    .alignmentGuide(.firstTextBaseline) { $0[VerticalAlignment.center] }
            }
            .padding(.horizontal, Theme.Spacing.window)
            BusyBanner()
                .padding(.horizontal, Theme.Spacing.window)
            content
        }
        .padding(.top, Theme.Spacing.section)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .onBackdrop()
        .navigationTitle(title)
        // The page draws its own large title; the toolbar's copy would repeat it.
        .toolbar(removing: .title)
        // With no title or items left in it, the toolbar would otherwise paint
        // an opaque band across the top of the backdrop.
        .toolbarBackgroundVisibility(.hidden, for: .windowToolbar)
    }
}

extension Page where Actions == EmptyView {
    init(title: String, subtitle: String? = nil, @ViewBuilder content: () -> Content) {
        self.init(title: title, subtitle: subtitle, actions: { EmptyView() }, content: content)
    }
}

/// A scrolling column of a page's cards, inset like the header above it.
struct PageScroll<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        ScrollView {
            GlassEffectContainer(spacing: Theme.Spacing.cards) {
                VStack(alignment: .leading, spacing: Theme.Spacing.cards) { content }
                    .padding(.horizontal, Theme.Spacing.window)
                    .padding(.vertical, Tokens.Space.sm)
            }
        }
        .scrollContentBackground(.hidden)
    }
}

/// One client app in a list: icon, name, a second line, the version and an
/// accessory — a state badge or an action button.
struct PackageRow<Accessory: View>: View {
    let name: String
    let subtitle: String
    var version: String?
    @ViewBuilder var accessory: Accessory

    var body: some View {
        HStack(spacing: Tokens.Space.md) {
            AppIcon(name: name)
            VStack(alignment: .leading, spacing: Tokens.Space.xxs) {
                Text(name).textStyle(Tokens.Typography.headline).foregroundStyle(Tokens.Colors.Text.primary)
                Text(subtitle)
                    .textStyle(Tokens.Typography.callout)
                    .foregroundStyle(Tokens.Colors.Text.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: Tokens.Space.sm)
            if let version {
                Text(version).textStyle(Tokens.Typography.mono).foregroundStyle(Tokens.Colors.Text.secondary)
            }
            accessory
        }
        .padding(.horizontal, Tokens.Component.PackageRow.padding)
        .padding(.vertical, Tokens.Space.sm)
        .frame(minHeight: Tokens.Component.PackageRow.height)
        .contentShape(.rect)
        .glassSurface(.rect(cornerRadius: Theme.Radius.card), interactive: true)
        .liftShadow()
    }
}

/// A client app's icon: two letters on a gradient, since ketch has no icon
/// for most command-line tools.
struct AppIcon: View {
    enum Size {
        /// Rows, the sheet, the menu bar.
        case row
        /// The package detail header.
        case hero

        var side: CGFloat { self == .row ? Tokens.Size.appIcon : Tokens.Size.appIcon * 2 }
        var radius: CGFloat { self == .row ? Theme.Radius.control : Theme.Radius.panel }
        var type: KetchTypeStyle { self == .row ? Tokens.Typography.headline : Tokens.Typography.title }
    }

    let name: String
    var size: Size = .row

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: size.radius, style: .continuous)
        Text(monogram)
            .textStyle(size.type)
            .foregroundStyle(Tokens.Colors.Accent.on)
            .frame(width: size.side, height: size.side)
            .background {
                shape.fill(
                    LinearGradient(
                        colors: Theme.iconColors(for: name), startPoint: .topLeading, endPoint: .bottomTrailing))
            }
            .overlay { shape.stroke(Tokens.Colors.Glass.highlight, lineWidth: Tokens.Size.hairline) }
            .liftShadow()
            .accessibilityHidden(true)
    }

    private var monogram: String { String(name.prefix(2)) }
}

/// A short state word in a capsule, on its status pair.
struct StatusBadge: View {
    let tone: StatusTone
    let label: String

    var body: some View {
        Text(label)
            .textStyle(Tokens.Typography.badge)
            .foregroundStyle(tone.color)
            .lineLimit(1)
            .padding(.horizontal, Tokens.Component.Badge.padding)
            .frame(minHeight: Tokens.Component.Badge.height)
            .background(tone.subtle, in: .capsule)
    }
}

/// A status glyph in a round well: the log's and the findings' leading icon.
struct StatusGlyph: View {
    let tone: StatusTone
    let symbol: String

    var body: some View {
        Image(systemName: symbol)
            .font(Tokens.Typography.badge.font)
            .foregroundStyle(tone.color)
            .frame(width: Tokens.Size.iconMd + Tokens.Space.xs, height: Tokens.Size.iconMd + Tokens.Space.xs)
            .background(tone.subtle, in: .circle)
            .accessibilityHidden(true)
    }
}

/// The design's Tint button: an accent-tinted glass capsule for the small
/// per-row actions (Get, Update), quieter than the prominent glass button.
extension View {
    /// A filled glass button in the error colour, for the one action that
    /// removes something.
    func destructiveProminent() -> some View {
        modifier(DestructiveProminent())
    }
}

private struct DestructiveProminent: ViewModifier {
    @Environment(\.colorSchemeContrast) private var contrast

    func body(content: Content) -> some View {
        content.buttonStyle(.glassProminent).tint(fill)
    }

    /// Status.error as the light appearance draws it. The token is an ink: in
    /// dark mode it lightens to stay legible as text, and the button's white
    /// label is not legible on that pink. The light ink carries white in both
    /// modes. The token is an AppKit dynamic colour, so the SwiftUI colour
    /// scheme does not pick its variant; the appearance has to.
    private var fill: Color {
        let ink = NSColor(Tokens.Colors.Status.error)
        let name: NSAppearance.Name = contrast == .increased ? .accessibilityHighContrastAqua : .aqua
        var resolved = ink
        NSAppearance(named: name)?.performAsCurrentDrawingAppearance {
            resolved = ink.usingColorSpace(.sRGB) ?? ink
        }
        return Color(nsColor: resolved)
    }
}

struct TintButtonStyle: ButtonStyle {
    @Environment(\.appearance) private var appearance
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(Tokens.Typography.callout.font.weight(.semibold))
            .foregroundStyle(isEnabled ? ink : Tokens.Colors.Text.tertiary)
            .padding(.horizontal, Tokens.Space.md)
            .padding(.vertical, Tokens.Space.xs)
            .glassSurface(
                .capsule, level: .elevated,
                tint: appearance.accentColor.opacity(Tokens.Opacity.tint), interactive: isEnabled)
    }

    /// `accent.ink` is the default accent deepened for text; a chosen accent
    /// has no deepened twin, so it is used as it is.
    private var ink: Color { appearance.accent.color ?? Tokens.Colors.Accent.ink }
}

extension ButtonStyle where Self == TintButtonStyle {
    static var tint: TintButtonStyle { TintButtonStyle() }
}

/// A glass search field: the design's Search field, in a page header.
struct SearchField: View {
    let prompt: String
    @Binding var text: String

    var body: some View {
        HStack(spacing: Tokens.Space.sm) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(Tokens.Colors.Text.secondary)
                .accessibilityHidden(true)
            TextField(prompt, text: $text)
                .textFieldStyle(.plain)
                .textStyle(Tokens.Typography.body)
                // The prompt is only a placeholder, which VoiceOver reads as a value.
                .accessibilityLabel(prompt)
            if !text.isEmpty {
                Button("Clear", systemImage: "xmark.circle.fill") { text = "" }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.plain)
                    .foregroundStyle(Tokens.Colors.Text.secondary)
            }
        }
        .padding(.horizontal, Tokens.Space.md)
        .frame(height: Tokens.Size.controlHeight + Tokens.Space.xs)
        // The mock's field is about a sidebar wide.
        .frame(width: Tokens.Size.sidebarWidth)
        .glassSurface(.capsule, level: .elevated, interactive: true)
    }
}

/// Another ketch process holds the lock: who, and a way to try again.
struct BusyBanner: View {
    @Environment(KetchStore.self) private var store

    var body: some View {
        if let busy = store.busy {
            HStack(spacing: Tokens.Space.sm) {
                Circle().fill(Tokens.Colors.Status.warning)
                    .frame(width: Tokens.Space.sm, height: Tokens.Space.sm)
                    .accessibilityHidden(true)
                Text("Another ketch is running (pid \(String(busy.pid))). Retry when it finishes.")
                    .textStyle(Tokens.Typography.body)
                    .foregroundStyle(Tokens.Colors.Text.primary)
                Spacer(minLength: Tokens.Space.sm)
                Button("Retry now") { Task { await store.retryBusy() } }
                    .buttonStyle(.glass)
                    .controlSize(.small)
                    .accessibilityIdentifier("busy-retry")
                Button("Dismiss", systemImage: "xmark") { store.busy = nil }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.plain)
                    .foregroundStyle(Tokens.Colors.Text.secondary)
            }
            .padding(.leading, Tokens.Space.lg)
            .padding(.trailing, Tokens.Space.sm)
            .padding(.vertical, Tokens.Space.xs)
            .frame(minHeight: Tokens.Size.controlHeight + Tokens.Space.md)
            .glassSurface(.rect(cornerRadius: Theme.Radius.card), tint: Tokens.Colors.Status.warningSubtle)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("busy-banner")
        }
    }
}

/// The running operation as a capsule along the bottom of the window, so it
/// stays in sight whichever screen is open.
struct ActivityBar: View {
    @Environment(KetchStore.self) private var store
    let activity: Activity

    var body: some View {
        HStack(spacing: Tokens.Space.md) {
            Text(line)
                .textStyle(Tokens.Typography.body)
                .foregroundStyle(Tokens.Colors.Text.primary)
                .lineLimit(1)
            Spacer(minLength: Tokens.Space.sm)
            ProgressView(value: activity.fraction)
                .frame(maxWidth: Tokens.Size.sidebarWidth)
                .accessibilityLabel(activity.title)
            Text(activity.fraction, format: .percent.precision(.fractionLength(0)))
                .textStyle(Tokens.Typography.mono)
                .foregroundStyle(Tokens.Colors.Text.secondary)
                .monospacedDigit()
            Button(activity.isCancelling ? "Cancelling…" : "Cancel", systemImage: "xmark.circle.fill") {
                store.cancel()
            }
            .labelStyle(.iconOnly)
            .buttonStyle(.plain)
            .foregroundStyle(Tokens.Colors.Text.secondary)
            .disabled(activity.isCancelling)
        }
        .padding(.horizontal, Tokens.Space.lg)
        .frame(height: Tokens.Size.controlHeight + Tokens.Space.md)
        .glassSurface(.rect(cornerRadius: Theme.Radius.panel), level: .elevated)
        .liftShadow()
        .padding(.horizontal, Theme.Spacing.window)
        .padding(.bottom, Theme.Spacing.section)
    }

    private var line: String {
        [activity.title, activity.status].compactMap { $0 }.joined(separator: " · ")
    }
}

/// A shelf or group label: the design's overline, upper-cased.
struct SectionLabel: View {
    let text: String

    var body: some View {
        Text(text)
            .textStyle(Tokens.Typography.overline)
            .textCase(.uppercase)
            .foregroundStyle(Tokens.Colors.Text.secondary)
    }
}
