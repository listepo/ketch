// What the user may change about the app's look, and what the app draws once
// the system's accessibility settings have had their say. Liquid Glass stays
// the system material, so the choices are the ones it allows: a tint, regular
// or clear glass, the accent, and the strength of the backdrop wash. Plain
// values, so the rules are tested without a window; Views/Glass.swift applies
// the resolved value through the `appearance` environment value.

import AppKit
import SwiftUI

/// The user's choices in Settings -> Appearance, and after `resolved(…)` what
/// the views draw.
struct Appearance: Hashable, Sendable {
    var tint: TintChoice = .standard
    var glassStyle: GlassStyle = .regular
    var accent: AccentChoice = .standard
    /// Opacity of the backdrop wash over the canvas, 0...`Tokens.Opacity.washMax`.
    var wash: Double = Tokens.Opacity.wash

    static let standard = Appearance()

    /// The appearance to draw. Reduce Transparency and Increase Contrast always
    /// win: under either there is no tint, no clear glass and no wash, and
    /// Increase Contrast also hands the accent back to the system, whose
    /// high-contrast variant is legible where a custom colour may not be.
    func resolved(reduceTransparency: Bool, increaseContrast: Bool) -> Appearance {
        var out = self
        out.wash = Self.clampWash(wash)
        if reduceTransparency || increaseContrast {
            out.tint = .standard
            out.glassStyle = .regular
            out.wash = 0
        }
        if increaseContrast {
            out.accent = .standard
        }
        return out
    }

    /// `wash` limited to what the setting allows; the contrast check holds up
    /// to `Tokens.Opacity.washMax` and no further.
    static func clampWash(_ wash: Double) -> Double {
        guard wash.isFinite else { return Tokens.Opacity.wash }
        return min(max(wash, 0), Tokens.Opacity.washMax)
    }

    /// The Liquid Glass variant for surfaces, tinted when a tint is chosen.
    func glass(dark: Bool) -> Glass {
        let base: Glass = glassStyle == .clear ? .clear : .regular
        guard let color = tint.color else { return base }
        return base.tint(color.opacity(dark ? Tokens.Opacity.tintDark : Tokens.Opacity.tint))
    }

    /// The accent to draw with: the chosen colour, or the system accent.
    var accentColor: Color { accent.color ?? .accentColor }
}

enum GlassStyle: String, CaseIterable, Identifiable, Sendable {
    case regular, clear

    var id: Self { self }
    var title: String { rawValue.capitalized }
}

/// A named swatch with a token colour.
protocol ColorPreset: RawRepresentable<String>, CaseIterable, Hashable, Sendable where AllCases == [Self] {
    /// What `ColorChoice.standard` is stored and shown as.
    static var standardKey: String { get }
    var color: Color { get }
}

extension ColorPreset {
    var title: String { rawValue.capitalized }
}

enum TintPreset: String, ColorPreset {
    case sky, mint, sand, rose, lilac, smoke

    static let standardKey = "clear"

    var color: Color {
        switch self {
        case .sky: Tokens.Colors.Preset.Tint.sky
        case .mint: Tokens.Colors.Preset.Tint.mint
        case .sand: Tokens.Colors.Preset.Tint.sand
        case .rose: Tokens.Colors.Preset.Tint.rose
        case .lilac: Tokens.Colors.Preset.Tint.lilac
        case .smoke: Tokens.Colors.Preset.Tint.smoke
        }
    }
}

enum AccentPreset: String, ColorPreset {
    case blue, purple, pink, red, orange, yellow, green, graphite

    static let standardKey = "system"

    var color: Color {
        switch self {
        case .blue: Tokens.Colors.Preset.Accent.blue
        case .purple: Tokens.Colors.Preset.Accent.purple
        case .pink: Tokens.Colors.Preset.Accent.pink
        case .red: Tokens.Colors.Preset.Accent.red
        case .orange: Tokens.Colors.Preset.Accent.orange
        case .yellow: Tokens.Colors.Preset.Accent.yellow
        case .green: Tokens.Colors.Preset.Accent.green
        case .graphite: Tokens.Colors.Preset.Accent.graphite
        }
    }
}

/// No colour (Clear for the tint, System for the accent), a preset, or a colour
/// the user picked.
enum ColorChoice<Preset: ColorPreset>: Hashable, Sendable {
    case standard
    case preset(Preset)
    case custom(RGBColor)

    /// Parses what `stored` wrote; anything else is the standard choice, so a
    /// hand-edited or older preference never breaks the window.
    init(stored: String?) {
        guard let stored, stored != Preset.standardKey else {
            self = .standard
            return
        }
        if let preset = Preset(rawValue: stored) {
            self = .preset(preset)
        } else if let rgb = RGBColor(hex: stored) {
            self = .custom(rgb)
        } else {
            self = .standard
        }
    }

    /// The UserDefaults form: the standard key, a preset's name, or `#rrggbb`.
    var stored: String {
        switch self {
        case .standard: Preset.standardKey
        case .preset(let preset): preset.rawValue
        case .custom(let rgb): rgb.hex
        }
    }

    /// `nil` for the standard choice.
    var color: Color? {
        switch self {
        case .standard: nil
        case .preset(let preset): preset.color
        case .custom(let rgb): rgb.color
        }
    }
}

typealias TintChoice = ColorChoice<TintPreset>
typealias AccentChoice = ColorChoice<AccentPreset>

/// An opaque sRGB colour the user picked, kept as 8-bit components so it
/// survives a round trip through `#rrggbb` unchanged.
struct RGBColor: Hashable, Sendable {
    let red: UInt8
    let green: UInt8
    let blue: UInt8

    init(red: UInt8, green: UInt8, blue: UInt8) {
        self.red = red
        self.green = green
        self.blue = blue
    }

    /// `#rrggbb`, case-insensitive; nothing else.
    init?(hex: String) {
        let digits = hex.dropFirst()
        guard hex.first == "#", digits.count == 6, digits.allSatisfy(\.isHexDigit),
            let value = UInt32(digits, radix: 16)
        else { return nil }
        self.init(red: UInt8(value >> 16 & 0xff), green: UInt8(value >> 8 & 0xff), blue: UInt8(value & 0xff))
    }

    /// The sRGB form of a picked colour; `nil` when it has no sRGB equivalent.
    init?(_ color: Color) {
        guard let srgb = NSColor(color).usingColorSpace(.sRGB) else { return nil }
        func byte(_ c: CGFloat) -> UInt8 { UInt8((min(max(c, 0), 1) * 255).rounded()) }
        self.init(red: byte(srgb.redComponent), green: byte(srgb.greenComponent), blue: byte(srgb.blueComponent))
    }

    var hex: String { String(format: "#%02x%02x%02x", red, green, blue) }

    var color: Color {
        Color(.sRGB, red: Double(red) / 255, green: Double(green) / 255, blue: Double(blue) / 255)
    }
}
