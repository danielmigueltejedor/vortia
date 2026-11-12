import Foundation
import Observation
import VortiaKit

/// What the interface draws and what it asks the engine for.
///
/// **A mirror, not the source of truth.** The engine owns the project, the case and the run;
/// this holds what it last said. Edits to the setup are sent back (validated, then saved) after
/// a short pause, so typing in a field is not a request per keystroke.
@MainActor
@Observable
public final class VortiaStore {
    public enum Connection: Equatable, Sendable {
        case starting
        case ready
        case lost(String)
    }

    public enum Stage: String, CaseIterable, Identifiable, Sendable {
        case geometry, mesh, physics, time, run, results, engineering
        public var id: String { rawValue }
    }

    public enum Job: Equatable, Sendable {
        case idle
        case running(phase: String)
        case finished(phase: String, ok: Bool, summary: String)

        public var isRunning: Bool {
            if case .running = self { return true }
            return false
        }
    }

    /// One residual at one moment of the run, for the chart.
    public struct ResidualPoint: Identifiable, Equatable, Sendable {
        public var id: Int
        public var time: Double
        public var field: String
        public var value: Double
    }

    public private(set) var connection: Connection = .starting
    public private(set) var engineVersion = ""
    public private(set) var openFoam: OpenFoamStatus?
    public private(set) var project: ProjectInfo?
    public private(set) var validation: Validation?
    public private(set) var job: Job = .idle
    public private(set) var latest: Progress?
    public private(set) var residuals: [ResidualPoint] = []
    public private(set) var log = ""
    public private(set) var results: ResultsInfo?
    public private(set) var statistics: FieldStatistics?
    public private(set) var slice: FieldTriangles?
    public private(set) var profile: LineProfile?
    public private(set) var error: String?
    /// The section to analyse, and the report of the last one.
    public var section = SectionOptions()
    public var sectionLabel = ""
    public private(set) var sectionReport: SectionReport?
    public private(set) var analysing = false
    public var stage: Stage = .geometry

    /// The setup being edited. Every change is validated and saved after a pause.
    public var setup: CaseSetup? {
        didSet {
            guard setup != oldValue, !applyingFromEngine, setup != nil else { return }
            scheduleSync()
        }
    }

    public var field: String? {
        didSet { if field != oldValue { Task { await loadField() } } }
    }
    /// 0, 1 or 2: the axis the slice is normal to.
    public var sliceAxis = 2 {
        didSet { if sliceAxis != oldValue { Task { await loadSlice() } } }
    }
    /// Where along that axis, from 0 to 1 of the domain.
    public var sliceFraction = 0.5

    @ObservationIgnored private var client: (any EngineClient)?
    @ObservationIgnored private var applyingFromEngine = false
    @ObservationIgnored private var sync: Task<Void, Never>?
    @ObservationIgnored private var residualCount = 0
    /// The log keeps its tail: a long run writes megabytes nobody scrolls back through.
    private static let logLimit = 120_000

    public init() {}

    // MARK: Connection

    public func attach(_ client: any EngineClient) async {
        self.client = client
        do {
            let hello = try await client.call(.hello, as: Hello.self)
            engineVersion = hello.version
            openFoam = hello.openfoam
            if let project = hello.project { adopt(project) }
            job = hello.running ? .running(phase: "solve") : job
            connection = .ready
        } catch {
            connection = .lost(error.localizedDescription)
        }
    }

    public func detach(reason: String) {
        client = nil
        connection = .lost(reason)
        if job.isRunning {
            job = .finished(phase: "solve", ok: false, summary: reason)
        }
    }

    public func connectionAttempt() {
        connection = .starting
    }

    public func dismissError() {
        error = nil
    }

    // MARK: Events

    public func apply(_ event: EngineEvent) {
        switch event {
        case .ready:
            break
        case .log(let text):
            log += text
            if log.count > Self.logLimit { log = String(log.suffix(Self.logLimit * 3 / 4)) }
        case .progress(let progress):
            latest = progress
            let time = progress.time ?? Double(residualCount)
            for residual in progress.residuals {
                guard let value = residual.initial, value > 0 else { continue }
                residualCount += 1
                residuals.append(
                    ResidualPoint(id: residualCount, time: time, field: residual.field, value: value))
            }
        case .job(let event):
            if event.state == "running" {
                job = .running(phase: event.phase)
            } else {
                job = .finished(
                    phase: event.phase, ok: event.ok ?? false, summary: event.summary ?? "")
                Task {
                    await refreshProject()
                    if event.ok == true { await loadResults() }
                }
            }
        }
    }

    // MARK: Projects

    public func create(at url: URL) async {
        await perform {
            let name = url.deletingPathExtension().lastPathComponent
            let project = try await $0.call(
                .create(path: url.path, name: name), as: ProjectInfo.self)
            self.adopt(project)
        }
    }

    public func open(_ url: URL) async {
        await perform {
            let project = try await $0.call(.open(path: url.path), as: ProjectInfo.self)
            self.adopt(project)
            await self.loadResults(quietly: true)
        }
    }

    /// Saves now rather than after the pause: before a run, or when asked.
    public func save() async {
        sync?.cancel()
        guard let setup, project != nil else { return }
        await perform {
            self.validation = try await $0.call(.validate(setup: setup), as: Validation.self)
            _ = try await $0.send(.save(setup: setup))
        }
    }

    public func detect() async {
        await perform { self.openFoam = try await $0.call(.detect, as: OpenFoamStatus.self) }
    }

    private func adopt(_ project: ProjectInfo) {
        self.project = project
        applyingFromEngine = true
        setup = project.setup
        applyingFromEngine = false
        results = nil
        statistics = nil
        slice = nil
        sectionReport = nil
        sectionLabel = project.name
        profile = nil
        residuals = []
        latest = nil
        log = ""
        job = .idle
        stage = project.state == "solved" ? .results : .geometry
        Task { await validateNow() }
    }

    private func refreshProject() async {
        guard let client else { return }
        if let hello = try? await client.call(.hello, as: Hello.self), let project = hello.project {
            self.project = project
        }
    }

    // MARK: Validating and saving

    private func scheduleSync() {
        sync?.cancel()
        sync = Task {
            try? await Task.sleep(for: .milliseconds(400))
            guard !Task.isCancelled else { return }
            await validateNow()
            if !job.isRunning, let setup, let client {
                _ = try? await client.send(.save(setup: setup))
            }
        }
    }

    private func validateNow() async {
        guard let setup, let client else { return }
        validation = try? await client.call(.validate(setup: setup), as: Validation.self)
    }

    // MARK: Running

    public var canRun: Bool {
        connection == .ready && project != nil && !job.isRunning && validation?.hasErrors != true
    }

    public func mesh() async {
        await start(.mesh)
    }

    public func solve() async {
        await start(.solve(processes: 1))
    }

    public func cancel() async {
        guard let client else { return }
        _ = try? await client.send(.cancel)
    }

    private func start(_ request: EngineRequest) async {
        await save()
        guard canRun else { return }
        residuals = []
        residualCount = 0
        latest = nil
        log = ""
        stage = .run
        await perform {
            _ = try await $0.send(request)
            self.job = .running(phase: request == .mesh ? "mesh" : "solve")
        }
    }

    // MARK: Results

    public func loadResults(quietly: Bool = false) async {
        guard let client else { return }
        do {
            let info = try await client.call(.results, as: ResultsInfo.self)
            results = info
            let preferred = ["U", "p"].first(where: info.fields.contains) ?? info.fields.first
            if field == nil || !(info.fields.contains(field ?? "")) {
                field = preferred
            } else {
                await loadField()
            }
        } catch {
            results = nil
            if !quietly { self.error = error.localizedDescription }
        }
    }

    private func loadField() async {
        guard let field, let client, results != nil else { return }
        statistics = try? await client.call(.statistics(field: field), as: FieldStatistics.self)
        await loadSlice()
        await loadProfile()
    }

    /// Measures the section through the latest result and writes its engineering report.
    public func analyseSection() async {
        guard let client, results != nil else { return }
        analysing = true
        defer { analysing = false }
        do {
            let label = sectionLabel.isEmpty ? (project?.name ?? "Section") : sectionLabel
            let data = try await client.send(.section(options: section, label: label))
            sectionReport = try SectionReport.decode(data)
        } catch {
            self.error = error.localizedDescription
        }
    }

    /// A section through the middle of the domain, normal to the chosen axis.
    public func centreSection() {
        guard let results, results.bounds.count == 2 else { return }
        let axis = section.axis
        section.position = (results.bounds[0][safe: axis] + results.bounds[1][safe: axis]) / 2
    }

    public func loadSlice() async {
        guard let field, let client, let results, results.bounds.count == 2 else { return }
        let low = results.bounds[0][safe: sliceAxis]
        let high = results.bounds[1][safe: sliceAxis]
        let position = low + (high - low) * min(max(sliceFraction, 0.001), 0.999)
        slice = try? await client.call(
            .slice(field: field, axis: sliceAxis, position: position), as: FieldTriangles.self)
    }

    /// The vertical centre line of the domain, the profile a cavity or a channel is judged by.
    private func loadProfile() async {
        guard let field, let client, let results, results.bounds.count == 2 else { return }
        let low = results.bounds[0]
        let high = results.bounds[1]
        let middle = (0..<3).map { (low[safe: $0] + high[safe: $0]) / 2 }
        let start = [middle[0], low[safe: 1], middle[2]]
        let end = [middle[0], high[safe: 1], middle[2]]
        profile = try? await client.call(
            .line(field: field, start: start, end: end), as: LineProfile.self)
    }

    // MARK: Plumbing

    private func perform(_ work: @MainActor (any EngineClient) async throws -> Void) async {
        guard let client else {
            error = L10n.t(.engineMissing)
            return
        }
        do {
            try await work(client)
        } catch {
            self.error = error.localizedDescription
        }
    }
}
