// Settings -> Appearance: the defaults, what survives a relaunch, what a
// damaged preference falls back to, and how Reduce Transparency and Increase
// Contrast override the user's choice.

import Foundation
import SwiftUI
import Testing

@testable import Ketch

@MainActor
@Suite struct AppearanceTests {
    @Test func freshSettingsUseTheStandardAppearance() {
        let settings = AppSettings(defaults: scratchDefaults())

        #expect(settings.appearance == .standard)
        #expect(settings.appearance.tint == .standard)
        #expect(settings.appearance.glassStyle == .regular)
        #expect(settings.appearance.accent == .standard)
        #expect(settings.appearance.wash == Tokens.Opacity.wash)
    }

    @Test func everyChoiceSurvivesARelaunch() {
        let defaults = scratchDefaults()
        let chosen = Appearance(
            tint: .preset(.mint),
            glassStyle: .clear,
            accent: .custom(RGBColor(red: 0x12, green: 0xab, blue: 0xef)),
            wash: Tokens.Opacity.washMax / 2
        )
        AppSettings(defaults: defaults).appearance = chosen

        #expect(AppSettings(defaults: defaults).appearance == chosen)
    }

    @Test func aCustomTintAndAPresetAccentSurviveARelaunch() {
        let defaults = scratchDefaults()
        let chosen = Appearance(tint: .custom(RGBColor(red: 1, green: 2, blue: 3)), accent: .preset(.graphite))
        AppSettings(defaults: defaults).appearance = chosen

        #expect(AppSettings(defaults: defaults).appearance == chosen)
        #expect(defaults.string(forKey: "appearanceTint") == "#010203")
        #expect(defaults.string(forKey: "appearanceAccent") == "graphite")
    }

    @Test func unreadablePreferencesFallBackToTheDefaults() {
        let defaults = scratchDefaults()
        defaults.set("tartan", forKey: "appearanceTint")
        defaults.set("frosted", forKey: "appearanceGlassStyle")
        defaults.set("#12345", forKey: "appearanceAccent")
        defaults.set("loud", forKey: "appearanceWash")

        #expect(AppSettings(defaults: defaults).appearance == .standard)
    }

    @Test func aStoredWashAboveTheMaximumIsClamped() {
        let defaults = scratchDefaults()
        defaults.set(0.9, forKey: "appearanceWash")

        #expect(AppSettings(defaults: defaults).appearance.wash == Tokens.Opacity.washMax)
    }

    @Test func withoutAccessibilitySettingsTheChoiceIsDrawnAsIs() {
        let chosen = Appearance(tint: .preset(.sand), glassStyle: .clear, accent: .preset(.pink), wash: 0.1)

        #expect(chosen.resolved(reduceTransparency: false, increaseContrast: false) == chosen)
    }

    @Test func reduceTransparencyDropsTintClearGlassAndWashButKeepsTheAccent() {
        let chosen = Appearance(tint: .preset(.sky), glassStyle: .clear, accent: .preset(.green), wash: 0.1)

        let drawn = chosen.resolved(reduceTransparency: true, increaseContrast: false)

        #expect(drawn == Appearance(tint: .standard, glassStyle: .regular, accent: .preset(.green), wash: 0))
    }

    @Test func increaseContrastAlsoHandsTheAccentBackToTheSystem() {
        let chosen = Appearance(
            tint: .custom(RGBColor(red: 9, green: 9, blue: 9)), glassStyle: .clear, accent: .preset(.yellow), wash: 0.2)

        #expect(chosen.resolved(reduceTransparency: false, increaseContrast: true) == Appearance(wash: 0))
        #expect(chosen.resolved(reduceTransparency: true, increaseContrast: true) == Appearance(wash: 0))
    }

    @Test func resolvingClampsAWashOutsideTheAllowedRange() {
        #expect(Appearance(wash: -1).resolved(reduceTransparency: false, increaseContrast: false).wash == 0)
        #expect(
            Appearance(wash: 5).resolved(reduceTransparency: false, increaseContrast: false).wash
                == Tokens.Opacity.washMax)
        #expect(Appearance.clampWash(.nan) == Tokens.Opacity.wash)
    }

    @Test func hexColoursRoundTripAndRejectAnythingElse() {
        #expect(RGBColor(hex: "#0A84FF")?.hex == "#0a84ff")
        #expect(RGBColor(hex: "0a84ff") == nil)
        #expect(RGBColor(hex: "#0a84f") == nil)
        #expect(RGBColor(hex: "#+a84ff") == nil)
        #expect(RGBColor(hex: "#0a84fg") == nil)
    }

    @Test func aPickedColourBecomesItsSRGBBytes() {
        #expect(RGBColor(Color(.sRGB, red: 1, green: 0.5, blue: 0))?.hex == "#ff8000")
    }

    @Test func theStandardChoicesAreStoredByName() {
        #expect(TintChoice.standard.stored == "clear")
        #expect(AccentChoice.standard.stored == "system")
        #expect(TintChoice(stored: "clear") == .standard)
        #expect(AccentChoice(stored: "system") == .standard)
        #expect(AccentChoice(stored: nil) == .standard)
    }
}
