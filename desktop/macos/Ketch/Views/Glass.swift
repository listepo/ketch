// The app's look: macOS 26 Liquid Glass from the system (`glassEffect`,
// `GlassEffectContainer`, the glass button styles), with depth from the
// backdrop wash behind the glass and a soft shadow under it. Only system
// materials are used, so Reduce Transparency and Increase Contrast keep working
// without code here; on top of that, `ketchAppearance()` drops the user's tint,
// clear glass and wash under either (Store/Appearance.swift).

import SwiftUI

extension EnvironmentValues {
    /// The resolved Settings -> Appearance, set once at each scene's root by
    /// `ketchAppearance()`.
    @Entry var appearance: Appearance = .standard
}

extension View {
    /// Applies Settings -> Appearance to a scene: resolves the user's choice
    /// against the accessibility settings, publishes it as `appearance`, and
    /// tints controls with the accent. Needs `AppSettings` in the environment.
    func ketchAppearance() -> some View {
        modifier(AppearanceRoot())
    }

    /// A floating glass card: the content padded, on the chosen glass in a
    /// rounded rectangle, lifted off the backdrop by a shadow.
    func glassCard(cornerRadius: CGFloat = Theme.Radius.card, interactive: Bool = false) -> some View {
        modifier(GlassCard(cornerRadius: cornerRadius, interactive: interactive))
    }

    /// Puts the section's content over the app backdrop, so the glass has
    /// something to refract.
    func onBackdrop() -> some View {
        background(Backdrop())
    }
}

private struct AppearanceRoot: ViewModifier {
    @Environment(AppSettings.self) private var settings
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast

    func body(content: Content) -> some View {
        let appearance = settings.appearance.resolved(
            reduceTransparency: reduceTransparency, increaseContrast: contrast == .increased)
        content
            .environment(\.appearance, appearance)
            .tint(appearance.accent.color)
    }
}

private struct GlassCard: ViewModifier {
    let cornerRadius: CGFloat
    let interactive: Bool
    @Environment(\.appearance) private var appearance
    @Environment(\.colorScheme) private var colorScheme

    func body(content: Content) -> some View {
        content
            .padding(Theme.Spacing.cardPadding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .glassEffect(
                appearance.glass(dark: colorScheme == .dark).interactive(interactive),
                in: .rect(cornerRadius: cornerRadius)
            )
            .shadow(color: Theme.Shadow.color, radius: Theme.Shadow.radius, y: Theme.Shadow.y)
    }
}

/// The canvas under everything in a section: the base colour with the
/// backdrop wash over it at the chosen strength. Colour, not a glass
/// imitation: the glass on top does the refraction.
struct Backdrop: View {
    @Environment(\.appearance) private var appearance

    var body: some View {
        ZStack {
            Tokens.Colors.Background.base
            if appearance.wash > 0 {
                ZStack {
                    LinearGradient(
                        colors: [Tokens.Colors.Background.washStart, Tokens.Colors.Background.washEnd],
                        startPoint: .topLeading, endPoint: .bottomTrailing
                    )
                    Hill.near.fill(Tokens.Colors.Background.washHill)
                    Hill.far.fill(Tokens.Colors.Background.washDeep)
                }
                .opacity(appearance.wash)
            }
        }
        .ignoresSafeArea()
    }
}

/// One hill along the bottom of the wash, filled down to the bottom edge: the
/// mock's Ocean wallpaper outlines (`M0 170 Q100 110 200 160 T400 140` and
/// `M0 205 Q120 165 240 205 T400 195` on 400×260), as fractions of the frame.
private struct Hill: Shape {
    /// The start, then (control, end) per quadratic segment.
    let start: CGPoint
    let curves: [(control: CGPoint, end: CGPoint)]

    static let near = Hill(
        start: CGPoint(x: 0, y: 0.654),
        curves: [
            (CGPoint(x: 0.25, y: 0.423), CGPoint(x: 0.5, y: 0.615)),
            (CGPoint(x: 0.75, y: 0.808), CGPoint(x: 1, y: 0.538)),
        ])
    static let far = Hill(
        start: CGPoint(x: 0, y: 0.788),
        curves: [
            (CGPoint(x: 0.3, y: 0.635), CGPoint(x: 0.6, y: 0.788)),
            (CGPoint(x: 0.9, y: 0.942), CGPoint(x: 1, y: 0.75)),
        ])

    func path(in rect: CGRect) -> Path {
        func at(_ p: CGPoint) -> CGPoint { CGPoint(x: rect.minX + p.x * rect.width, y: rect.minY + p.y * rect.height) }
        var path = Path()
        path.move(to: at(start))
        for curve in curves {
            path.addQuadCurve(to: at(curve.end), control: at(curve.control))
        }
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.maxY))
        path.closeSubpath()
        return path
    }
}
