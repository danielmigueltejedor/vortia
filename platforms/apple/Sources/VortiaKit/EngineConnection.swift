import Foundation

/// Where requests go. The app hands the store a running engine; a test hands it a script.
public protocol EngineClient: Sendable {
    /// Sends `request` and waits for its answer: the `result` as raw JSON, or the engine's error.
    func send(_ request: EngineRequest) async throws -> Data
}

/// vortia-engine as a child process, spoken to over its stdin and stdout.
///
/// Answers are matched to requests by id, so several can be in flight; events arrive on
/// `events` in the order the engine wrote them. When the process ends, every waiting request
/// fails and `events` finishes.
public final class EngineConnection: EngineClient, @unchecked Sendable {
    public let events: AsyncStream<EngineEvent>

    private let process = Process()
    private let input = Pipe()
    private let output = Pipe()
    private let continuation: AsyncStream<EngineEvent>.Continuation
    private let lock = NSLock()
    private var splitter = LineSplitter()
    private var nextId = 1
    private var waiting: [Int: CheckedContinuation<Data, any Error>] = [:]
    private var ended = false

    /// Starts the engine. `environment` replaces the process's own when given.
    public init(executable: URL, environment: [String: String]? = nil) throws {
        (events, continuation) = AsyncStream.makeStream(of: EngineEvent.self)
        process.executableURL = executable
        process.standardInput = input
        process.standardOutput = output
        process.standardError = FileHandle.standardError
        if let environment { process.environment = environment }
        output.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let chunk = handle.availableData
            if chunk.isEmpty {
                handle.readabilityHandler = nil
                self?.finish(EngineError("the engine stopped"))
            } else {
                self?.receive(chunk)
            }
        }
        process.terminationHandler = { [weak self] process in
            self?.finish(EngineError("the engine stopped (status \(process.terminationStatus))"))
        }
        try process.run()
    }

    public func send(_ request: EngineRequest) async throws -> Data {
        try await withCheckedThrowingContinuation { (waiter: CheckedContinuation<Data, any Error>) in
            lock.lock()
            guard !ended else {
                lock.unlock()
                waiter.resume(throwing: EngineError("the engine is not running"))
                return
            }
            let id = nextId
            nextId += 1
            let line: Data
            do {
                line = try request.line(id: id)
            } catch {
                lock.unlock()
                waiter.resume(throwing: error)
                return
            }
            waiting[id] = waiter
            lock.unlock()
            do {
                try input.fileHandleForWriting.write(contentsOf: line)
            } catch {
                resolve(id, .failure(EngineError("the engine is not listening")))
            }
        }
    }

    /// Asks the engine to stop whatever runs and exit, then makes sure it does.
    public func stop() {
        Task { _ = try? await send(.quit) }
        DispatchQueue.global().asyncAfter(deadline: .now() + 3) { [process] in
            if process.isRunning { process.terminate() }
        }
    }

    private func receive(_ chunk: Data) {
        lock.lock()
        let lines = splitter.feed(chunk)
        lock.unlock()
        for line in lines {
            switch Wire.parse(line) {
            case .reply(let id, let result): resolve(id, result.mapError { $0 as any Error })
            case .event(let event): continuation.yield(event)
            case .unreadable: break
            }
        }
    }

    private func resolve(_ id: Int, _ result: Result<Data, any Error>) {
        lock.lock()
        let waiter = waiting.removeValue(forKey: id)
        lock.unlock()
        waiter?.resume(with: result)
    }

    private func finish(_ error: EngineError) {
        lock.lock()
        guard !ended else {
            lock.unlock()
            return
        }
        ended = true
        let all = waiting
        waiting = [:]
        lock.unlock()
        for waiter in all.values { waiter.resume(throwing: error) }
        continuation.finish()
    }
}

extension EngineClient {
    /// Sends `request` and decodes its answer as `type`.
    public func call<T: Decodable>(_ request: EngineRequest, as type: T.Type) async throws -> T {
        let data = try await send(request)
        do {
            return try JSONDecoder().decode(T.self, from: data)
        } catch {
            throw EngineError("the engine answered \(request.command) in a form this app cannot read")
        }
    }
}
