import Charts
import SwiftUI
import VortiaKit

// MARK: Run

/// The calculation stage: where the run is, the residuals falling, and the log underneath.
struct CalculationMonitor: View {
    @Environment(VortiaStore.self) private var store

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                status.card()
                if let convergence = store.convergence {
                    ConvergenceCard(convergence: convergence).card()
                }
                ResidualChart(
                    points: store.residuals,
                    xTitle: store.isFlowProject ? L10n.t(.iteration) : L10n.t(.simulatedTime))
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
                    if store.isFlowProject, let iteration = store.latest?.time,
                        let limit = store.flow?["solver"]["iterations"].double, limit > 0
                    {
                        ProgressView(value: min(iteration / limit, 1)) {
                            Text("\(L10n.t(.iteration)) \(Int(iteration).formatted()) / \(Int(limit).formatted())")
                                .font(.caption).monospacedDigit()
                        }
                    } else if let time = store.latest?.time, let end = store.setup?.time.endTime, end > 0,
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
    var xTitle = L10n.t(.simulatedTime)

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n.t(.residuals)).font(.headline)
            if points.isEmpty {
                Text("—").foregroundStyle(.secondary).frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                Chart(thinned) { point in
                    LineMark(
                        x: .value(xTitle, point.time),
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
