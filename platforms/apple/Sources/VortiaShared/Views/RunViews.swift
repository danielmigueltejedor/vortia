import Charts
import SwiftUI
import VortiaKit

// MARK: Run

/// The run as it happens: where it is, the residuals falling, and the log underneath.
struct RunView: View {
    @Environment(VortiaStore.self) private var store

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                status.card()
                if let convergence = store.convergence {
                    ConvergenceCard(convergence: convergence).card()
                }
                ResidualChart(points: store.residuals)
                    .frame(height: 280)
                    .card()
                LogView(text: store.log)
                    .frame(minHeight: 200)
                    .card()
            }
            .padding(20)
        }
    }

    private var status: some View {
        HStack(alignment: .center, spacing: 16) {
            switch store.job {
            case .idle:
                Image(systemName: "play.circle").font(.largeTitle).foregroundStyle(.tint)
                Text(L10n.t(.idle)).font(.title3.weight(.semibold))
            case .running(let phase):
                ProgressView().controlSize(.large)
                VStack(alignment: .leading, spacing: 4) {
                    Text(phase == "mesh" ? L10n.t(.meshing) : L10n.t(.solving))
                        .font(.title3.weight(.semibold))
                    if let time = store.latest?.time, let end = store.setup?.time.endTime, end > 0,
                        !(store.setup?.isSteady ?? false)
                    {
                        ProgressView(value: min(time / end, 1)) {
                            Text("\(L10n.t(.simulatedTime)): \(time.scientific) / \(end.scientific) s")
                                .font(.caption).monospacedDigit()
                        }
                    }
                }
            case .finished(_, let ok, let summary):
                Image(systemName: ok ? "checkmark.circle.fill" : "exclamationmark.octagon.fill")
                    .font(.largeTitle)
                    .foregroundStyle(ok ? .green : .red)
                VStack(alignment: .leading, spacing: 2) {
                    Text(ok ? L10n.t(.finished) : L10n.t(.failed)).font(.title3.weight(.semibold))
                    Text(summary).foregroundStyle(.secondary).textSelection(.enabled)
                }
            }
            Spacer()
            if let courant = store.latest?.courantMax, courant > 0 {
                VStack(alignment: .trailing) {
                    Text(L10n.t(.courant)).font(.caption).foregroundStyle(.secondary)
                    Text(courant.scientific).font(.title3.monospacedDigit())
                }
            }
            RunControls().buttonStyle(.borderedProminent).labelStyle(.titleAndIcon)
        }
    }
}

/// Initial residual of every field over the run, on a log scale: the curve an engineer reads
/// convergence from.
struct ResidualChart: View {
    var points: [VortiaStore.ResidualPoint]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n.t(.residuals)).font(.headline)
            if points.isEmpty {
                Text("—").foregroundStyle(.secondary).frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                Chart(thinned) { point in
                    LineMark(
                        x: .value(L10n.t(.simulatedTime), point.time),
                        y: .value(L10n.t(.residuals), point.value)
                    )
                    .foregroundStyle(by: .value(L10n.t(.field), point.field))
                    .interpolationMethod(.linear)
                }
                .chartYScale(type: .log)
                .chartLegend(position: .top, alignment: .trailing)
            }
        }
    }

    /// A long run has hundreds of thousands of samples; a chart a few hundred points wide
    /// needs no more than about a thousand per field.
    private var thinned: [VortiaStore.ResidualPoint] {
        guard points.count > 4000 else { return points }
        let stride = points.count / 4000 + 1
        return points.enumerated().compactMap { $0.offset % stride == 0 ? $0.element : nil }
    }
}

struct LogView: View {
    var text: String

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n.t(.log)).font(.headline)
            ScrollViewReader { proxy in
                ScrollView {
                    Text(text.isEmpty ? "—" : text)
                        .font(.system(.caption, design: .monospaced))
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Color.clear.frame(height: 1).id("end")
                }
                .frame(minHeight: 180)
                .onChange(of: text.count) { proxy.scrollTo("end", anchor: .bottom) }
            }
        }
    }
}

// MARK: Results

/// The latest solution: a slice coloured by the chosen field, its statistics and its profile.
struct ResultsView: View {
    @Environment(VortiaStore.self) private var store

    var body: some View {
        if let results = store.results {
            content(results)
        } else {
            EmptyState(
                symbol: "chart.xyaxis.line", title: L10n.t(.noResults),
                message: L10n.t(.noResultsBody))
        }
    }

    private func content(_ results: ResultsInfo) -> some View {
        @Bindable var store = store
        return ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    Picker(L10n.t(.field), selection: $store.field) {
                        ForEach(results.fields, id: \.self) { Text($0).tag(Optional($0)) }
                    }
                    .frame(maxWidth: 220)
                    Picker(L10n.t(.axis), selection: $store.sliceAxis) {
                        Text("x").tag(0)
                        Text("y").tag(1)
                        Text("z").tag(2)
                    }
                    .pickerStyle(.segmented)
                    .frame(maxWidth: 160)
                    Slider(value: $store.sliceFraction, in: 0...1) {
                        Text(L10n.t(.position))
                    } onEditingChanged: { editing in
                        if !editing { Task { await store.loadSlice() } }
                    }
                }
                HStack(alignment: .top, spacing: 16) {
                    SliceView(slice: store.slice, axis: store.sliceAxis)
                        .frame(minHeight: 360)
                        .card()
                    if let statistics = store.statistics {
                        StatisticsCard(statistics: statistics).frame(width: 240).card()
                    }
                }
                if let profile = store.profile {
                    ProfileChart(profile: profile, field: store.field ?? "")
                        .frame(height: 240)
                        .card()
                }
            }
            .padding(20)
        }
    }
}

/// A slice drawn flat, each triangle in the colour of its mean value. The axis the slice is
/// normal to is dropped; the other two become the picture's x and y.
struct SliceView: View {
    var slice: FieldTriangles?
    var axis: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let slice, slice.triangleCount > 0, let range = slice.range, range.count == 2 {
                Canvas { context, size in draw(slice, range: range, in: &context, size: size) }
                    .aspectRatio(aspect(slice), contentMode: .fit)
                ColorLegend(range: range[0]...max(range[1], range[0]))
                    .frame(maxWidth: 360)
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }

    private var planeAxes: (Int, Int) {
        switch axis {
        case 0: (1, 2)
        case 1: (0, 2)
        default: (0, 1)
        }
    }

    private func extent(_ slice: FieldTriangles) -> (CGRect) {
        let (a, b) = planeAxes
        var minA = Float.infinity, maxA = -Float.infinity, minB = Float.infinity, maxB = -Float.infinity
        for vertex in stride(from: 0, to: slice.positions.count, by: 3) {
            minA = min(minA, slice.positions[vertex + a])
            maxA = max(maxA, slice.positions[vertex + a])
            minB = min(minB, slice.positions[vertex + b])
            maxB = max(maxB, slice.positions[vertex + b])
        }
        return CGRect(
            x: Double(minA), y: Double(minB), width: Double(max(maxA - minA, 1e-12)),
            height: Double(max(maxB - minB, 1e-12)))
    }

    private func aspect(_ slice: FieldTriangles) -> CGFloat {
        let box = extent(slice)
        return min(max(box.width / box.height, 0.2), 5)
    }

    private func draw(
        _ slice: FieldTriangles, range: [Double], in context: inout GraphicsContext, size: CGSize
    ) {
        let (a, b) = planeAxes
        let box = extent(slice)
        let scale = min(size.width / box.width, size.height / box.height)
        let offset = CGPoint(
            x: (size.width - box.width * scale) / 2, y: (size.height - box.height * scale) / 2)
        let span = max(range[1] - range[0], 1e-30)
        func point(_ vertex: Int) -> CGPoint {
            let i = vertex * 3
            return CGPoint(
                x: offset.x + (Double(slice.positions[i + a]) - box.minX) * scale,
                y: size.height - offset.y - (Double(slice.positions[i + b]) - box.minY) * scale)
        }
        for triangle in 0..<slice.triangleCount {
            let v = triangle * 3
            var path = Path()
            path.move(to: point(v))
            path.addLine(to: point(v + 1))
            path.addLine(to: point(v + 2))
            path.closeSubpath()
            let mean = Double(slice.values[v] + slice.values[v + 1] + slice.values[v + 2]) / 3
            let color = Colormap.color((mean - range[0]) / span)
            context.fill(path, with: .color(color))
            // A hairline of the same colour hides the seams antialiasing leaves between cells.
            context.stroke(path, with: .color(color), lineWidth: 0.6)
        }
    }
}

struct StatisticsCard: View {
    var statistics: FieldStatistics

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L10n.t(.statistics)).font(.headline)
            row(L10n.t(.minimum), statistics.minimum)
            row(L10n.t(.maximum), statistics.maximum)
            row(L10n.t(.mean), statistics.mean)
            row(L10n.t(.standardDeviation), statistics.standardDeviation)
            LabeledContent(L10n.t(.cells), value: statistics.cells.formatted())
                .monospacedDigit()
            if !statistics.histogram.isEmpty {
                Chart(Array(statistics.histogram.enumerated()), id: \.offset) { item in
                    BarMark(
                        x: .value(L10n.t(.value), item.offset),
                        y: .value(L10n.t(.cells), item.element.cells))
                    .foregroundStyle(
                        Colormap.color(Double(item.offset) / Double(max(statistics.histogram.count - 1, 1))))
                }
                .chartXAxis(.hidden)
                .frame(height: 90)
            }
        }
    }

    private func row(_ title: String, _ value: Double?) -> some View {
        LabeledContent(title, value: value?.scientific ?? "—").monospacedDigit()
    }
}

struct ProfileChart: View {
    var profile: LineProfile
    var field: String

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n.t(.profile)).font(.headline)
            Chart(Array(profile.samples.enumerated()), id: \.offset) { item in
                if let distance = item.element.distance, let value = item.element.value {
                    LineMark(
                        x: .value(L10n.t(.value), value),
                        y: .value(L10n.t(.distance), distance))
                    .foregroundStyle(.tint)
                }
            }
            .chartXAxisLabel(field)
            .chartYAxisLabel(L10n.t(.distance))
        }
    }
}
