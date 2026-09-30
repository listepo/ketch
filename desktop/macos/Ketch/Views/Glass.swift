// The app's look: macOS 26 Liquid Glass from the system (`glassEffect`,
// `GlassEffectContainer`, the glass button styles), with depth from a tinted
// backdrop behind the glass and a soft shadow under it. Only system materials
// are used, so Reduce Transparency and Increase Contrast keep working without
// code here.

import SwiftUI

extension View {
    /// A floating glass card: the content padded, on regular glass in a
    /// rounded rectangle, lifted off the backdrop by a shadow.
    func glassCard(cornerRadius: CGFloat = Theme.Radius.card, interactive: Bool = false) -> some View {
        padding(Theme.Spacing.cardPadding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .glassEffect(.regular.interactive(interactive), in: .rect(cornerRadius: cornerRadius))
            .shadow(color: Theme.Shadow.color, radius: Theme.Shadow.radius, y: Theme.Shadow.y)
    }

    /// Puts the section's content over the app backdrop, so the glass has
    /// something to refract.
    func onBackdrop() -> some View {
        background(Backdrop())
    }
}

/// A soft wash of the accent colour, drawn under everything in a section.
/// Colour, not a glass imitation: the glass on top does the refraction.
struct Backdrop: View {
    var body: some View {
        LinearGradient(
            colors: Theme.Palette.backdrop,
            startPoint: .topLeading, endPoint: .bottomTrailing
        )
        .ignoresSafeArea()
    }
}
