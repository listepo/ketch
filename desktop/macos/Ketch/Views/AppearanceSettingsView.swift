// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// Settings -> Appearance: the window tint, regular or clear glass, the accent
// and the strength of the backdrop wash. Every change applies at once through
// `AppSettings.appearance`; the system's Reduce Transparency and Increase
// Contrast override the choices, and the page says so while they do.

import SwiftUI

struct AppearanceSettingsView: View {
    @Environment(AppSettings.self) private var settings
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast

    var body: some View {
        @Bindable var settings = settings
        Form {
            LabeledContent("Tint") {
                Swatches(choice: $settings.appearance.tint, standardTitle: "Clear", customTitle: "Custom tint")
            }
            .accessibilityIdentifier("appearance-tint")
            Picker("Glass", selection: $settings.appearance.glassStyle) {
                ForEach(GlassStyle.allCases) { style in
                    Text(style.title).tag(style)
                }
            }
            .pickerStyle(.segmented)
            LabeledContent("Accent") {
                Swatches(choice: $settings.appearance.accent, standardTitle: "System", customTitle: "Custom accent")
            }
            .accessibilityIdentifier("appearance-accent")
            LabeledContent("Backdrop wash") {
                HStack {
                    Slider(value: $settings.appearance.wash, in: 0...Tokens.Opacity.washMax)
                        .frame(width: 180)
                        // The raw value is a fraction of full opacity; the label beside it is the share the setting allows.
                        .accessibilityLabel("Backdrop wash")
                        .accessibilityValue(percent(settings.appearance.wash))
                    Text(percent(settings.appearance.wash))
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                        .frame(width: 44, alignment: .trailing)
                }
            }

            if let override {
                Text(override).font(.caption).foregroundStyle(.secondary)
            }
            HStack {
                Text("Changes apply at once and are kept on this Mac.")
                    .font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button("Reset") { settings.appearance = .standard }
                    .disabled(settings.appearance == .standard)
            }
        }
        .formStyle(.grouped)
    }

    /// The wash as a share of the strongest the setting allows.
    private func percent(_ wash: Double) -> String {
        "\(Int((wash / Tokens.Opacity.washMax * 100).rounded()))%"
    }

    private var override: String? {
        if contrast == .increased {
            return "Increase Contrast is on in System Settings: tint, clear glass, accent and wash are paused."
        }
        if reduceTransparency {
            return "Reduce Transparency is on in System Settings: tint, clear glass and wash are paused."
        }
        return nil
    }
}

/// The standard choice, each preset, and a colour well for a custom colour, as
/// a row of round swatches.
private struct Swatches<Preset: ColorPreset>: View {
    @Binding var choice: ColorChoice<Preset>
    let standardTitle: String
    /// Names the colour well for VoiceOver; the two rows would otherwise both say "Custom".
    let customTitle: String

    var body: some View {
        HStack(spacing: Tokens.Space.sm) {
            Swatch(title: standardTitle, fill: nil, isOn: choice == .standard) { choice = .standard }
            ForEach(Preset.allCases, id: \.self) { preset in
                Swatch(title: preset.title, fill: preset.color, isOn: choice == .preset(preset)) {
                    choice = .preset(preset)
                }
            }
            ColorPicker(customTitle, selection: custom, supportsOpacity: false)
                .labelsHidden()
                .help(customTitle)
                .overlay {
                    if case .custom = choice {
                        Circle().strokeBorder(.primary, lineWidth: 2).padding(-4).allowsHitTesting(false)
                    }
                }
        }
    }

    /// The colour well's binding: shows the custom colour (or white), and
    /// picking a colour makes it the custom choice.
    private var custom: Binding<Color> {
        Binding(
            get: {
                if case .custom(let rgb) = choice { return rgb.color }
                return .white
            },
            set: { color in
                if let rgb = RGBColor(color) { choice = .custom(rgb) }
            }
        )
    }
}

private struct Swatch: View {
    /// The mock's swatch: 22 pt, with room for the selection ring around it.
    private static let size: CGFloat = 22

    let title: String
    /// `nil` draws the standard choice: a slashed circle for "none".
    let fill: Color?
    let isOn: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            ZStack {
                if let fill {
                    Circle().fill(fill)
                } else {
                    Circle().fill(.background)
                    Image(systemName: "circle.slash").foregroundStyle(.secondary)
                }
                Circle().strokeBorder(.separator, lineWidth: 1)
            }
            .frame(width: Self.size, height: Self.size)
            .padding(Tokens.Space.xxs)
            .overlay {
                if isOn { Circle().strokeBorder(.primary, lineWidth: 2) }
            }
        }
        .buttonStyle(.plain)
        .help(title)
        .accessibilityLabel(title)
        .accessibilityAddTraits(isOn ? .isSelected : [])
    }
}
