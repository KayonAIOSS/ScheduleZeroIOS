import SwiftUI

/// Same colors as the Android Material theme (ui/theme/Theme.kt).
struct Palette {
    let primary: Color
    let onPrimary: Color
    let primaryContainer: Color
    let onPrimaryContainer: Color
    let secondary: Color
    let secondaryContainer: Color
    let onSecondaryContainer: Color
    let background: Color
    let onBackground: Color
    let onSurface: Color
    let onSurfaceVariant: Color
    let surfaceContainer: Color
    let surfaceContainerHigh: Color
    let surfaceContainerHighest: Color
    let outline: Color
    let outlineVariant: Color
    let error: Color
    let warning: Color

    // Dark: pitch black + purple highlights + white text.
    static let dark = Palette(
        primary: Color(hex: 0xB388FF),
        onPrimary: Color(hex: 0x000000),
        primaryContainer: Color(hex: 0x2A1748),
        onPrimaryContainer: Color(hex: 0xEADDFF),
        secondary: Color(hex: 0xD0BCFF),
        secondaryContainer: Color(hex: 0x231B30),
        onSecondaryContainer: Color(hex: 0xEADDFF),
        background: Color(hex: 0x000000),
        onBackground: Color(hex: 0xFFFFFF),
        onSurface: Color(hex: 0xFFFFFF),
        onSurfaceVariant: Color(hex: 0xB9B3C6),
        surfaceContainer: Color(hex: 0x0E0C13),
        surfaceContainerHigh: Color(hex: 0x15121B),
        surfaceContainerHighest: Color(hex: 0x1C1824),
        outline: Color(hex: 0x4A4257),
        outlineVariant: Color(hex: 0x2A2433),
        error: Color(hex: 0xFF6E7F),
        warning: Color(hex: 0xFFB74D)
    )

    // Light: purple swapped for cyan / light blue.
    static let light = Palette(
        primary: Color(hex: 0x0091BE),
        onPrimary: Color(hex: 0xFFFFFF),
        primaryContainer: Color(hex: 0xCCF2FF),
        onPrimaryContainer: Color(hex: 0x00313F),
        secondary: Color(hex: 0x00A6C8),
        secondaryContainer: Color(hex: 0xDDF6FC),
        onSecondaryContainer: Color(hex: 0x00313F),
        background: Color(hex: 0xF6FBFD),
        onBackground: Color(hex: 0x0E1A1F),
        onSurface: Color(hex: 0x0E1A1F),
        onSurfaceVariant: Color(hex: 0x4B5E66),
        surfaceContainer: Color(hex: 0xFFFFFF),
        surfaceContainerHigh: Color(hex: 0xE9F3F7),
        surfaceContainerHighest: Color(hex: 0xDDEBF0),
        outline: Color(hex: 0x8AA0A8),
        outlineVariant: Color(hex: 0xCFDDE2),
        error: Color(hex: 0xC62839),
        warning: Color(hex: 0xB35C00)
    )
}

private struct PaletteKey: EnvironmentKey {
    static let defaultValue = Palette.dark
}

extension EnvironmentValues {
    var palette: Palette {
        get { self[PaletteKey.self] }
        set { self[PaletteKey.self] = newValue }
    }
}

/// Material-like type scale.
extension Font {
    static let szDisplay = Font.system(size: 45, weight: .bold).leading(.tight)
    static let szHeadline = Font.system(size: 28, weight: .semibold)
    static let szHeadlineSmall = Font.system(size: 24, weight: .semibold)
    static let szTitleLarge = Font.system(size: 22, weight: .semibold)
    static let szTitle = Font.system(size: 16, weight: .semibold)
    static let szBodyLarge = Font.system(size: 16)
    static let szBody = Font.system(size: 14)
    static let szBodySmall = Font.system(size: 12)
    static let szLabel = Font.system(size: 11, weight: .medium)
}

/// Small caps-style label ("TODAY", "THIS WEEK"), 1pt letter spacing like Android's labelSmall.
struct LabelText: View {
    let text: String
    var color: Color? = nil
    @Environment(\.palette) var p

    init(_ text: String, color: Color? = nil) {
        self.text = text
        self.color = color
    }

    var body: some View {
        Text(text.uppercased())
            .font(.szLabel)
            .tracking(1)
            .foregroundStyle(color ?? p.primary)
    }
}

/// Rounded card with optional border, matching the Android cards.
struct CardBox<Content: View>: View {
    var radius: CGFloat = 20
    var fill: Color? = nil
    var border: Color? = nil
    var borderWidth: CGFloat = 1
    @ViewBuilder var content: () -> Content
    @Environment(\.palette) var p

    var body: some View {
        content()
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(fill ?? p.surfaceContainer, in: RoundedRectangle(cornerRadius: radius, style: .continuous))
            .overlay {
                if let border {
                    RoundedRectangle(cornerRadius: radius, style: .continuous).strokeBorder(border, lineWidth: borderWidth)
                }
            }
    }
}

/// Filled / tonal / outlined / text buttons in the Material style.
struct SZButtonStyle: ButtonStyle {
    enum Kind { case filled, tonal, outlined, text }
    var kind: Kind = .filled
    @Environment(\.palette) var p
    @Environment(\.isEnabled) var enabled

    func makeBody(configuration: Configuration) -> some View {
        let (bg, fg, stroke): (Color, Color, Color?) = {
            switch kind {
            case .filled: return (p.primary, p.onPrimary, nil)
            case .tonal: return (p.secondaryContainer, p.onSecondaryContainer, nil)
            case .outlined: return (.clear, p.primary, p.outline)
            case .text: return (.clear, p.primary, nil)
            }
        }()
        configuration.label
            .font(.system(size: 14, weight: .medium))
            .lineLimit(1)
            .padding(.horizontal, kind == .text ? 12 : 20)
            .frame(minHeight: 40)
            .foregroundStyle(fg)
            .background(Capsule().fill(bg))
            .overlay { if let stroke { Capsule().strokeBorder(stroke, lineWidth: 1) } }
            .opacity(enabled ? (configuration.isPressed ? 0.75 : 1) : 0.38)
            .contentShape(Capsule())
    }
}

extension ButtonStyle where Self == SZButtonStyle {
    static var szFilled: SZButtonStyle { SZButtonStyle(kind: .filled) }
    static var szTonal: SZButtonStyle { SZButtonStyle(kind: .tonal) }
    static var szOutlined: SZButtonStyle { SZButtonStyle(kind: .outlined) }
    static var szText: SZButtonStyle { SZButtonStyle(kind: .text) }
}
