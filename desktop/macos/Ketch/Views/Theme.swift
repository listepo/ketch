// Every colour, spacing, radius and shadow the views use, in one place. The
// design system (F14) generates tokens for these; switching to them is a
// change to this file only. Liquid Glass itself stays a system material and
// is not themed here.

import SwiftUI

enum Theme {
    enum Spacing {
        /// Between cards in a list.
        static let cards: CGFloat = 10
        /// Between blocks inside a section.
        static let section: CGFloat = 12
        /// Between the large cards of a detail page.
        static let page: CGFloat = 16
        /// Inside a card, around its content.
        static let cardPadding: CGFloat = 12
    }

    enum Radius {
        static let card: CGFloat = 16
        static let panel: CGFloat = 20
    }

    enum Shadow {
        static let color = Color.black.opacity(0.12)
        static let radius: CGFloat = 10
        static let y: CGFloat = 5
    }

    enum Palette {
        /// The wash behind the glass, top-leading to bottom-trailing.
        static let backdrop = [Color.accentColor.opacity(0.22), Color.purple.opacity(0.10), Color.clear]
        /// The tint of an "update available" badge.
        static let updateBadge = Color.accentColor.opacity(0.35)
        static let ok = Color.green
        static let warning = Color.orange
        static let error = Color.red
    }
}
