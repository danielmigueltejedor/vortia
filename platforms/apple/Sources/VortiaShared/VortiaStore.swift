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

    /// The six stages of a case, in the order Vortia lists them. A contour project and a box
    /// project share them; what each stage shows depends on which one is open.
    public enum Stage: String, CaseIterable, Identifiable, Sendable {
        case project, geometry, mesh, physics, calculation, results
        public var id: String { rawValue }
    }

    /// The line a profile is sampled along.
    public enum ProfileLine: String, CaseIterable, Identifiable, Sendable {
        /// Along X, just above the axis: how a nozzle or a duct is read.
        case axis
        /// Across Y, through the middle of the domain: how a cavity or a channel is read.
        case vertical
        public var id: String { rawValue }
    }

    /// A contour as the editor changes it: its vertices (mm) and each segment's bulge.
    public struct ContourShape: Equatable, Sendable {
        public var points: [[Double]]
        public var bulges: [Double]

        public init(points: [[Double]], bulges: [Double]) {
            self.points = points
            self.bulges = bulges
        }
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
    public private(set) var latest: VortiaKit.Progress?
    public private(set) var residuals: [ResidualPoint] = []
    public private(set) var log = ""
    public private(set) var results: ResultsInfo?
    public private(set) var statistics: FieldStatistics?
    public private(set) var slice: FieldTriangles?
    /// The outer surface of the domain, for the 3D view. Loaded only while that view shows.
    public private(set) var surface: FieldTriangles?
    public private(set) var profile: LineProfile?
    public private(set) var error: String?
    /// The fluid contour of a compressible project; nil for a box-domain case.
    public private(set) var sketch: Sketch?
    /// Where the compressible solver says it is on the way to convergence.
    public private(set) var convergence: Convergence?
    /// The compressible setup being edited, saved after a pause like the box setup.
    public var flow: JSONValue? {
        didSet {
            guard flow != oldValue, !applyingFromEngine, flow != nil else { return }
            scheduleFlowSave()
        }
    }
    @ObservationIgnored private var flowSync: Task<Void, Never>?

    /// A project with a contour runs the compressible workflow.
    public var isFlowProject: Bool { sketch != nil }
    /// The stages the sidebar lists, in order: always the same six, as in Vortia.
    public var visibleStages: [Stage] { Stage.allCases }
    /// Earlier contours, for undo, and the ones undone, for redo.
    public private(set) var undoContours: [ContourShape] = []
    public private(set) var redoContours: [ContourShape] = []
    public var canUndoContour: Bool { !undoContours.isEmpty && !job.isRunning }
    public var canRedoContour: Bool { !redoContours.isEmpty && !job.isRunning }
    /// The section to analyse, and the report of the last one.
    public var section = SectionOptions()
    public var sectionLabel = ""
    public private(set) var sectionReport: SectionReport?
    public private(set) var analysing = false
    public var stage: Stage = .project
    /// Where the profile in Results is sampled.
    public var profileLine: ProfileLine = .vertical {
        didSet { if profileLine != oldValue { Task { await loadProfile() } } }
    }

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
    /// An axisymmetric slice drawn again below the axis, as the whole section reads.
    public var revolve = true
    /// The result in 3D (the domain's surface) rather than as a flat slice.
    public var threeD = false {
        didSet { if threeD && !oldValue { Task { await loadSurface() } } }
    }

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
            if let convergence = progress.convergence { self.convergence = convergence }
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

    /// A new project starts as Vortia's do: a fluid contour to draw, here the parametric
    /// nozzle, ready to be reshaped.
    public func create(at url: URL) async {
        await perform {
            let name = url.deletingPathExtension().lastPathComponent
            let project = try await $0.call(
                .create(path: url.path, name: name), as: ProjectInfo.self)
            self.adopt(project)
            struct Answer: Decodable { var sketch: Sketch }
            self.sketch = try await $0.call(.nozzle(NozzleDimensions()), as: Answer.self).sketch
            self.profileLine = .axis
            await self.loadFlow()
            self.stage = .geometry
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
        surface = nil
        profile = nil
        residuals = []
        latest = nil
        log = ""
        job = .idle
        sketch = nil
        flow = nil
        convergence = nil
        undoContours = []
        redoContours = []
        stage = project.state == "solved" ? .results : .project
        Task {
            await validateNow()
            await loadSketch()
        }
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
        guard connection == .ready, project != nil, !job.isRunning else { return false }
        return isFlowProject || validation?.hasErrors != true
    }

    public func mesh() async {
        await start(isFlowProject ? .flowMesh : .mesh)
    }

    public func solve() async {
        await start(isFlowProject ? .flowSolve : .solve(processes: 1))
    }

    // MARK: Contour and compressible flow

    private func loadSketch() async {
        guard let client else { return }
        struct Answer: Decodable { var sketch: Sketch? }
        guard let answer = try? await client.call(.sketch, as: Answer.self) else { return }
        sketch = answer.sketch
        if sketch != nil {
            profileLine = .axis
            await loadFlow()
        }
    }

    private func loadFlow() async {
        guard let client else { return }
        struct Answer: Decodable { var flow: JSONValue }
        guard let answer = try? await client.call(.flow, as: Answer.self) else { return }
        applyingFromEngine = true
        flow = answer.flow
        applyingFromEngine = false
    }

    /// Replaces the contour with the parametric nozzle of these dimensions.
    public func makeNozzle(_ dimensions: NozzleDimensions) async {
        struct Answer: Decodable { var sketch: Sketch }
        await perform {
            if let sketch = self.sketch { self.remember(sketch) }
            self.sketch = try await $0.call(.nozzle(dimensions), as: Answer.self).sketch
            self.profileLine = .axis
            await self.loadFlow()
            self.stage = .geometry
        }
    }

    /// Saves an edited contour, which can be undone; boundaries follow their segments in the
    /// engine.
    public func updateContour(points: [[Double]], closed: Bool, bulges: [Double]) async {
        guard let sketch else { return }
        let before = sketch
        if await sendContour(ContourShape(points: points, bulges: bulges), closed: closed) {
            remember(before)
        }
    }

    public func undoContour() async {
        guard let sketch, let previous = undoContours.last, !job.isRunning else { return }
        let current = ContourShape(points: sketch.points, bulges: sketch.bulges)
        if await sendContour(previous, closed: sketch.closed) {
            undoContours.removeLast()
            redoContours.append(current)
        }
    }

    public func redoContour() async {
        guard let sketch, let next = redoContours.last, !job.isRunning else { return }
        let current = ContourShape(points: sketch.points, bulges: sketch.bulges)
        if await sendContour(next, closed: sketch.closed) {
            redoContours.removeLast()
            undoContours.append(current)
        }
    }

    private func remember(_ sketch: Sketch) {
        undoContours.append(ContourShape(points: sketch.points, bulges: sketch.bulges))
        if undoContours.count > 100 { undoContours.removeFirst() }
        redoContours = []
    }

    private func sendContour(_ shape: ContourShape, closed: Bool) async -> Bool {
        guard let sketch else { return false }
        struct Answer: Decodable { var sketch: Sketch }
        let file = sketch.file(points: shape.points, closed: closed, bulges: shape.bulges)
        var saved = false
        await perform {
            self.sketch = try await $0.call(.saveSketch(file), as: Answer.self).sketch
            await self.loadFlow()
            saved = true
        }
        return saved
    }

    private func scheduleFlowSave() {
        flowSync?.cancel()
        flowSync = Task {
            try? await Task.sleep(for: .milliseconds(500))
            guard !Task.isCancelled, !job.isRunning, let flow, let client else { return }
            struct Answer: Decodable { var flow: JSONValue }
            do {
                let saved = try await client.call(.saveFlow(flow), as: Answer.self)
                // The engine normalises (names, limits); show what it kept.
                if saved.flow != flow {
                    applyingFromEngine = true
                    self.flow = saved.flow
                    applyingFromEngine = false
                }
            } catch {
                self.error = error.localizedDescription
            }
        }
    }

    public func cancel() async {
        guard let client else { return }
        _ = try? await client.send(.cancel)
    }

    private func start(_ request: EngineRequest) async {
        if isFlowProject {
            flowSync?.cancel()
            if let flow, let client { _ = try? await client.send(.saveFlow(flow)) }
        } else {
            await save()
        }
        guard canRun else { return }
        convergence = nil
        residuals = []
        residualCount = 0
        latest = nil
        log = ""
        stage = .calculation
        await perform {
            _ = try await $0.send(request)
            self.job = .running(phase: request == .mesh || request == .flowMesh ? "mesh" : "solve")
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
        if threeD { await loadSurface() }
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

    public func loadSurface() async {
        guard let field, let client, results != nil else { return }
        surface = try? await client.call(.surface(field: field), as: FieldTriangles.self)
    }

    public func loadSlice() async {
        guard let field, let client, let results, results.bounds.count == 2 else { return }
        let low = results.bounds[0][safe: sliceAxis]
        let high = results.bounds[1][safe: sliceAxis]
        let position = low + (high - low) * min(max(sliceFraction, 0.001), 0.999)
        slice = try? await client.call(
            .slice(field: field, axis: sliceAxis, position: position), as: FieldTriangles.self)
    }

    /// The chosen line through the latest result: along the axis, or vertically through the
    /// middle of the domain.
    private func loadProfile() async {
        guard let field, let client, let results, results.bounds.count == 2 else { return }
        let low = results.bounds[0]
        let high = results.bounds[1]
        let middle = (0..<3).map { (low[safe: $0] + high[safe: $0]) / 2 }
        let start: [Double]
        let end: [Double]
        switch profileLine {
        case .vertical:
            start = [middle[0], low[safe: 1], middle[2]]
            end = [middle[0], high[safe: 1], middle[2]]
        case .axis:
            // A hair above Y = 0, so the line stays inside the cells that touch the axis.
            let above = low[safe: 1] + (high[safe: 1] - low[safe: 1]) * 1e-3
            start = [low[safe: 0], above, middle[2]]
            end = [high[safe: 0], above, middle[2]]
        }
        profile = try? await client.call(
            .line(field: field, start: start, end: end, points: 200), as: LineProfile.self)
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
