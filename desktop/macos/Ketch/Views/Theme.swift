// The names the views use for spacing, radii, shadow and status colour, mapped
// onto the generated design tokens (design/generated/Tokens.swift, from
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
    }

    enum Radius {
        static let card = Tokens.Radius.md
        static let panel = Tokens.Radius.lg
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
}
