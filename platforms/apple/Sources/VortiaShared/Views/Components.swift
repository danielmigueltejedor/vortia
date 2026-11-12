import SwiftUI
import VortiaKit

// MARK: Numbers

/// A labelled number field that edits a Double in place, in the system's own style.
struct NumberField: View {
    var title: String
    @Binding var value: Double
    var format: FloatingPointFormatStyle<Double> = .number.precision(.significantDigits(1...6))

    var body: some View {
        TextField(title, value: $value, format: format)
            .multilineTextAlignment(.trailing)
            .monospacedDigit()
    }
}

/// Three numbers side by side: a point or a vector.
struct VectorField: View {
    var title: String
    @Binding var value: [Double]

    var body: some View {
        LabeledContent(title) {
            HStack(spacing: 6) {
                ForEach(0..<3, id: \.self) { axis in
                    TextField(
                        ["x", "y", "z"][axis],
                        value: Binding(
                            get: { value[safe: axis] },
                            set: { new in
                                var copy = value
                                while copy.count < 3 { copy.append(0) }
                                copy[axis] = new
                                value = copy
                            }),
                        format: .number.precision(.significantDigits(1...6))
                    )
                    .multilineTextAlignment(.trailing)
                    .monospacedDigit()
                    .frame(minWidth: 64)
                }
            }
        }
    }
}

extension Double {
    /// Compact scientific text for residuals, viscosities and the like.
    var scientific: String {
        guard isFinite else { return "—" }
        if self == 0 { return "0" }
        let magnitude = abs(self)
        if magnitude >= 1e-3 && magnitude < 1e5 {
            return formatted(.number.precision(.significantDigits(1...4)))
        }
        return String(format: "%.3g", self)
    }
}

// MARK: Colour

/// A perceptually ordered map from cool to warm, like the viridis family: dark violet, blue,
/// teal, green, yellow. It reads in both appearances and stays ordered in greyscale.
enum Colormap {
    private static let stops: [(Double, Double, Double)] = [
        (0.267, 0.005, 0.329), (0.283, 0.141, 0.458), (0.254, 0.265, 0.530),
        (0.207, 0.372, 0.553), (0.164, 0.471, 0.558), (0.128, 0.567, 0.551),
        (0.135, 0.659, 0.518), (0.267, 0.749, 0.441), (0.478, 0.821, 0.318),
        (0.741, 0.873, 0.150), (0.993, 0.906, 0.144),
    ]

    static func color(_ t: Double) -> Color {
        let (r, g, b) = rgb(t)
        return Color(red: r, green: g, blue: b)
    }

    static func rgb(_ t: Double) -> (Double, Double, Double) {
        let clamped = t.isFinite ? min(max(t, 0), 1) : 0
        let scaled = clamped * Double(stops.count - 1)
        let index = min(Int(scaled), stops.count - 2)
        let f = scaled - Double(index)
        let (a, b) = (stops[index], stops[index + 1])
        return (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f, a.2 + (b.2 - a.2) * f)
    }
}

/// The colour bar beside a field, with its two ends.
struct ColorLegend: View {
    var range: ClosedRange<Double>

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            LinearGradient(
                colors: stride(from: 0.0, through: 1.0, by: 0.1).map(Colormap.color),
                startPoint: .leading, endPoint: .trailing
            )
            .frame(height: 10)
            .clipShape(Capsule())
            HStack {
                Text(range.lowerBound.scientific)
                Spacer()
                Text(range.upperBound.scientific)
            }
            .font(.caption.monospacedDigit())
            .foregroundStyle(.secondary)
        }
    }
}

// MARK: Surfaces

extension View {
    /// A card of the system's glass on macOS 26, its ordinary grouped background before.
    @ViewBuilder
    func card() -> some View {
        #if compiler(>=6.2)
            if #available(macOS 26.0, *) {
                self.padding(16)
                    .glassEffect(.regular, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            } else {
                self.padding(16)
                    .background(.background.secondary, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            }
        #else
            self.padding(16)
                .background(.background.secondary, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        #endif
    }
}

/// A screen with nothing to show yet: a symbol, a title and what to do about it.
struct EmptyState<Actions: View>: View {
    var symbol: String
    var title: String
    var message: String?
    @ViewBuilder var actions: Actions

    var body: some View {
        VStack(spacing: 14) {
            Image(systemName: symbol)
                .font(.system(size: 44, weight: .light))
                .foregroundStyle(.tint)
            Text(title).font(.title2.weight(.semibold))
            if let message {
                Text(message)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 420)
            }
            actions
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

extension EmptyState where Actions == EmptyView {
    init(symbol: String, title: String, message: String? = nil) {
        self.init(symbol: symbol, title: title, message: message) { EmptyView() }
    }
}
