// The names the views use for spacing, radii, shadow, type and status colour,
// mapped onto the generated design tokens (design/generated/Tokens.swift, from
// design/tokens.json). Views keep these role names; which token a role takes
// is decided here, so a palette change never touches a view. Liquid Glass
// itself stays a system material; the user's tint, glass style, accent and wash
// arrive through the `appearance` environment value (Store/Appearance.swift).

import SwiftUI

enum Theme {
    enum Spacing {
        /// Between cards in a list.
        static let cards = Tokens.Space.sm
        /// Between blocks inside a section.
        static let section = Tokens.Space.md
        /// Between the large cards of a detail page.
        static let page = Tokens.Space.lg
        /// Inside a card, around its content.
        static let cardPadding = Tokens.Space.md
        /// Inside the larger panels: the hero, the detail and log cards.
        static let panelPadding = Tokens.Component.Card.padding
        /// Between the window's edge and a page's content (the mock's `pad/window`).
        static let window = Tokens.Space.xl
    }

    enum Radius {
        /// Package rows, findings, small cards (`radius/card`).
        static let card = Tokens.Radius.md
        /// The hero, the detail panels, the activity bar (`radius/panel`).
        static let panel = Tokens.Radius.lg
        /// Nav items and other small controls (`radius/control`).
        static let control = Tokens.Radius.sm
    }

    /// The outer layer of `elevation.lift`, under every glass card.
    enum Shadow {
        private static let drop = Tokens.Elevation.lift.last { !$0.inset }
        /// Softened: the token's CSS form has a negative spread, which SwiftUI
        /// shadows cannot express and which keeps most of the colour hidden
        /// under the card.
        static let color = (drop?.color ?? .clear).opacity(0.35)
        static let radius = drop?.radius ?? 0
        static let y = drop?.y ?? 0
    }

    enum Palette {
        static let ok = Tokens.Colors.Status.installed
        static let warning = Tokens.Colors.Status.warning
        static let error = Tokens.Colors.Status.error
    }

    /// A client app's monogram tile: two accent presets as a gradient, chosen
    /// by name so a package keeps its colours from launch to launch.
    static func iconColors(for name: String) -> [Color] {
        let pairs = iconPairs
        // `hashValue` is seeded per process, so it would repaint every launch.
        let index = name.unicodeScalars.reduce(0) { $0 &+ Int($1.value) } % pairs.count
        return [pairs[index].0, pairs[index].1]
    }

    private static let iconPairs: [(Color, Color)] = {
        typealias Accent = Tokens.Colors.Preset.Accent
        return [
            (Accent.orange, Accent.pink),
            (Accent.green, Accent.blue),
            (Accent.purple, Accent.blue),
            (Accent.yellow, Accent.orange),
            (Accent.graphite, Accent.graphite),
            (Accent.red, Accent.pink),
            (Accent.blue, Accent.purple),
            (Accent.green, Accent.green),
        ]
    }()
}

/// A state a badge, a log line or a finding shows, as a pair of status tokens:
/// the strong colour for text and glyphs, the subtle one for the well behind.
enum StatusTone {
    case installed, update, busy, warning, error

    var color: Color {
        switch self {
        case .installed: Tokens.Colors.Status.installed
        case .update: Tokens.Colors.Status.update
        case .busy: Tokens.Colors.Status.busy
        case .warning: Tokens.Colors.Status.warning
        case .error: Tokens.Colors.Status.error
        }
    }

    var subtle: Color {
        switch self {
        case .installed: Tokens.Colors.Status.installedSubtle
        case .update: Tokens.Colors.Status.updateSubtle
        case .busy: Tokens.Colors.Status.busySubtle
        case .warning: Tokens.Colors.Status.warningSubtle
        case .error: Tokens.Colors.Status.errorSubtle
        }
    }
}

extension View {
    /// One of the design's text styles: size, weight, tracking and leading.
    func textStyle(_ style: KetchTypeStyle) -> some View {
        font(style.font).tracking(style.tracking).lineSpacing(style.lineSpacing)
    }
}

extension String {
    /// A path as the user would type it, with `~` for their home.
    var abbreviatingHome: String { (self as NSString).abbreviatingWithTildeInPath }
}
