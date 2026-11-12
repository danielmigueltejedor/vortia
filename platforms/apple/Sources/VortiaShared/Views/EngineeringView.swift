import SwiftUI
import UniformTypeIdentifiers
import VortiaKit

/// A section through the latest result, measured: flows, mean values, the momentum and pressure
/// terms, and the report the desktop app writes, saved as HTML, CSV or JSON.
struct EngineeringView: View {
    @Environment(VortiaStore.self) private var store
    @State private var exporting: ExportedReport?

    var body: some View {
        if store.results == nil {
            EmptyState(
                symbol: "ruler", title: L10n.t(.noResults), message: L10n.t(.noResultsBody))
        } else {
            content
        }
    }

    private var content: some View {
        @Bindable var store = store
        return Form {
            Section(L10n.t(.engineering)) {
                TextField(L10n.t(.reportName), text: $store.sectionLabel)
                Picker(L10n.t(.axis), selection: $store.section.axis) {
                    Text("x").tag(0)
                    Text("y").tag(1)
                    Text("z").tag(2)
                }
                .pickerStyle(.segmented)
                HStack {
                    NumberField(title: L10n.t(.position), value: $store.section.position)
                    Button(L10n.t(.centre)) { store.centreSection() }
                }
                Picker(L10n.t(.direction), selection: $store.section.normal) {
                    Text("+").tag(1)
                    Text("−").tag(-1)
                }
                .pickerStyle(.segmented)
            }
            Section(L10n.t(.references)) {
                Toggle(L10n.t(.kinematicPressure), isOn: $store.section.kinematicPressure)
                Toggle(L10n.t(.useDensityField), isOn: $store.section.useDensityField)
                if !store.section.useDensityField {
                    NumberField(title: L10n.t(.constantDensity), value: $store.section.constantDensity)
                }
                NumberField(title: L10n.t(.referencePressure), value: $store.section.referencePressure)
                NumberField(title: L10n.t(.referenceDensity), value: $store.section.referenceDensity)
                NumberField(title: L10n.t(.referenceSpeed), value: $store.section.referenceSpeed)
                NumberField(title: L10n.t(.referenceArea), value: $store.section.referenceArea)
            }
            Section {
                HStack {
                    Button {
                        Task { await store.analyseSection() }
                    } label: {
                        Label(L10n.t(.analyse), systemImage: "ruler")
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(store.analysing || store.job.isRunning)
                    if store.analysing { ProgressView().controlSize(.small) }
                }
            }
            if let report = store.sectionReport {
                Section(report.report.payload.label) {
                    ForEach(SectionReport.metricKeys, id: \.self) { key in
                        LabeledContent(L10n.metric(key)) {
                            Text(report.report.payload.metrics[key]?.scientific ?? "—")
                                .monospacedDigit()
                                .textSelection(.enabled)
                        }
                    }
                    Text(L10n.t(.reportNote)).font(.caption).foregroundStyle(.secondary)
                    HStack {
                        Button("HTML") { exporting = .html(report) }
                        Button("CSV") { exporting = .csv(report) }
                        Button("JSON") { exporting = .json(report) }
                    }
                    Text("SHA-256 " + report.report.sha256)
                        .font(.caption2.monospaced())
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
            }
        }
        .formStyle(.grouped)
        .onAppear {
            if store.section.position == 0 { store.centreSection() }
        }
        .fileExporter(
            isPresented: Binding(get: { exporting != nil }, set: { if !$0 { exporting = nil } }),
            document: exporting?.document, contentType: exporting?.type ?? .html,
            defaultFilename: exporting?.filename
        ) { _ in exporting = nil }
    }
}

/// One of the three forms a report is saved in.
private enum ExportedReport {
    case html(SectionReport)
    case csv(SectionReport)
    case json(SectionReport)

    var type: UTType {
        switch self {
        case .html: .html
        case .csv: .commaSeparatedText
        case .json: .json
        }
    }

    var document: ReportDocument {
        switch self {
        case .html(let r): ReportDocument(data: Data(r.html.utf8))
        case .csv(let r): ReportDocument(data: Data(r.csv.utf8))
        case .json(let r): ReportDocument(data: r.json)
        }
    }

    var filename: String {
        let report: SectionReport
        switch self {
        case .html(let r), .csv(let r), .json(let r): report = r
        }
        return report.report.payload.label.isEmpty ? "section" : report.report.payload.label
    }
}

struct ReportDocument: FileDocument {
    static let readableContentTypes: [UTType] = [.html, .commaSeparatedText, .json]
    var data: Data

    init(data: Data) { self.data = data }

    init(configuration: ReadConfiguration) throws {
        data = configuration.file.regularFileContents ?? Data()
    }

    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        FileWrapper(regularFileWithContents: data)
    }
}
