import Foundation

/// Any JSON, for documents the app edits a few fields of and must otherwise carry intact: the
/// compressible flow setup has more keys than any screen shows, and losing one would change the
/// case.
public enum JSONValue: Codable, Equatable, Sendable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    public init(from decoder: any Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .null }
        else if let v = try? c.decode(Bool.self) { self = .bool(v) }
        else if let v = try? c.decode(Double.self) { self = .number(v) }
        else if let v = try? c.decode(String.self) { self = .string(v) }
        else if let v = try? c.decode([JSONValue].self) { self = .array(v) }
        else { self = .object(try c.decode([String: JSONValue].self)) }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .null: try c.encodeNil()
        case .bool(let v): try c.encode(v)
        case .number(let v): try c.encode(v)
        case .string(let v): try c.encode(v)
        case .array(let v): try c.encode(v)
        case .object(let v): try c.encode(v)
        }
    }

    public subscript(key: String) -> JSONValue {
        get {
            if case .object(let o) = self { return o[key] ?? .null }
            return .null
        }
        set {
            var o: [String: JSONValue] = [:]
            if case .object(let existing) = self { o = existing }
            o[key] = newValue
            self = .object(o)
        }
    }

    public subscript(index: Int) -> JSONValue {
        get {
            if case .array(let a) = self, a.indices.contains(index) { return a[index] }
            return .null
        }
        set {
            guard case .array(var a) = self, a.indices.contains(index) else { return }
            a[index] = newValue
            self = .array(a)
        }
    }

    public var double: Double? {
        if case .number(let v) = self { return v }
        return nil
    }

    public var string: String? {
        if case .string(let v) = self { return v }
        return nil
    }

    public var bool: Bool? {
        if case .bool(let v) = self { return v }
        return nil
    }

    public var array: [JSONValue] {
        if case .array(let v) = self { return v }
        return []
    }
}
