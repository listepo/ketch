// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The contract scenarios in `desktop/contract/scenarios/`, as Swift values.
//
// They are generated from `ketch-ffi`'s Rust types (`crates/ketch-ffi/tests/
// contract.rs`) and read by every app's fake core, so the fake speaks the
// records, event streams and errors the real core does. This file only
// decodes them and maps the wire shape onto the app's own types; the replay
// itself lives in `FakeKetchCore`.
//
// The mapping is where the app's protocol and the core's differ today (the
// protocol was written before `ketch-ffi`): stages are coarser, progress is
// keyed by task id, a missing lock holder has no pid. F12's live adapter makes
// the same translations, so a scenario that maps here maps there.

import Foundation

struct ContractScenario: Decodable, Sendable {
    struct Call: Decodable, Sendable {
        var operation: String
        var specs: [String]?
        var names: [String]?
        var package: String?
        var query: String?
    }

    struct Package: Decodable, Sendable {
        var name: String
        var version: String
        var source: String
        var prefix: String
    }

    struct Installed: Decodable, Sendable {
        var package: Package
        var replaced: String?
    }

    struct RegistryEntry: Decodable, Sendable {
        var name: String
        var source: String
        var description: String?
        var latest: String?
    }

    struct SearchResults: Decodable, Sendable {
        var known: [RegistryEntry]
    }

    struct UpgradeEntry: Decodable, Sendable {
        var name: String
        var installed: String
        var latest: String
        var pinned: Bool
        var heldBy: String?

        private enum CodingKeys: String, CodingKey {
            case name, installed, latest, pinned
            case heldBy = "held_by"
        }
    }

    struct ChangelogEntry: Decodable, Sendable {
        var version: String
        var heading: String?
        var body: String
    }

    struct CheckEntry: Decodable, Sendable {
        var name: String
        var outcome: String
        var detail: String
        var fix: String?
    }

    struct TaskKind: Decodable, Sendable {
        var type: String
        var label: String?
    }

    /// Every field of every event kind, optional: the `type` says which are set.
    struct Event: Decodable, Sendable {
        var type: String
        var package: String?
        var stage: String?
        var verb: String?
        var detail: String?
        var id: UInt64?
        var task: TaskKind?
        var name: String?
        var done: UInt64?
        var total: UInt64?
        var message: String?
    }

    struct Question: Decodable, Sendable {
        var type: String
        var package: String?
        var candidates: [String]?
    }

    enum Step: Decodable, Sendable {
        case event(Event)
        case ask(Question)

        private enum CodingKeys: String, CodingKey { case type, event, question }

        init(from decoder: any Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            switch try container.decode(String.self, forKey: .type) {
            case "event": self = .event(try container.decode(Event.self, forKey: .event))
            case "ask": self = .ask(try container.decode(Question.self, forKey: .question))
            case let other:
                throw DecodingError.dataCorruptedError(
                    forKey: .type, in: container, debugDescription: "unknown step type \(other)")
            }
        }
    }

    struct WireError: Decodable, Sendable {
        var type: String
        var pid: UInt32?
        var name: String?
        var message: String?

        /// The app's error for it. A `Busy` with no known holder has no pid to
        /// show, so it reads as pid 0.
        var asKetchError: KetchError {
            switch type {
            case "busy": .busy(pid: pid ?? 0)
            case "cancelled": .cancelled
            case "not_found": .notFound(name: name ?? "")
            case "network": .network(message: message ?? "")
            case "verification": .verification(message: message ?? "")
            default: .other(message: message ?? "")
            }
        }
    }

    enum Outcome: Sendable {
        case ok(Data)
        case error(WireError)
    }

    var name: String
    var description: String
    var call: Call
    var script: [Step]
    var outcome: Outcome

    private enum CodingKeys: String, CodingKey { case name, description, call, script, outcome }
    private enum OutcomeKeys: String, CodingKey { case type, value, error }

    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        name = try container.decode(String.self, forKey: .name)
        description = try container.decode(String.self, forKey: .description)
        call = try container.decode(Call.self, forKey: .call)
        script = try container.decode([Step].self, forKey: .script)
        let outcomeContainer = try container.nestedContainer(keyedBy: OutcomeKeys.self, forKey: .outcome)
        switch try outcomeContainer.decode(String.self, forKey: .type) {
        case "ok":
            // Kept as JSON until the call says what type the value is.
            let value = try outcomeContainer.decode(JSONValue.self, forKey: .value)
            outcome = .ok(try JSONEncoder().encode(value))
        case "error":
            outcome = .error(try outcomeContainer.decode(WireError.self, forKey: .error))
        case let other:
            throw DecodingError.dataCorruptedError(
                forKey: .type, in: outcomeContainer, debugDescription: "unknown outcome type \(other)")
        }
    }

    /// The returned value as `T`; throws when the scenario failed instead.
    func value<T: Decodable>(as type: T.Type) throws -> T {
        switch outcome {
        case .ok(let data): return try JSONDecoder().decode(T.self, from: data)
        case .error(let error): throw error.asKetchError
        }
    }

    /// Every scenario in `directory`, sorted by name.
    static func load(from directory: URL) throws -> [ContractScenario] {
        let files = try FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)
            .filter { $0.pathExtension == "json" }
            .sorted { $0.lastPathComponent < $1.lastPathComponent }
        return try files.map { try JSONDecoder().decode(ContractScenario.self, from: Data(contentsOf: $0)) }
    }
}

// MARK: Mapping to the app's types

extension ContractScenario.Package {
    var asInstalled: InstalledPackage {
        let (scheme, id) = ContractScenario.split(source)
        return InstalledPackage(
            name: name, version: version, source: scheme, repo: scheme == "github" ? id : nil,
            description: nil, path: prefix)
    }
}

extension ContractScenario.RegistryEntry {
    var asRegistryPackage: RegistryPackage {
        let (scheme, id) = ContractScenario.split(source)
        return RegistryPackage(
            name: name, repo: scheme == "github" ? id : source, description: description, latest: latest)
    }
}

extension ContractScenario.UpgradeEntry {
    var asUpgrade: Upgrade {
        Upgrade(name: name, from: installed, to: latest, heldBy: pinned ? (heldBy ?? "a pin") : nil)
    }
}

extension ContractScenario.CheckEntry {
    var asFinding: Finding {
        let severity: Finding.Severity =
            switch outcome {
            case "ok": .ok
            case "warn": .warning
            default: .error
            }
        return Finding(id: name, severity: severity, message: detail, fix: fix)
    }
}

extension ContractScenario {
    /// `github:owner/repo` as `("github", "owner/repo")`.
    static func split(_ source: String) -> (scheme: String, id: String) {
        guard let colon = source.firstIndex(of: ":") else { return (source, "") }
        return (String(source[..<colon]), String(source[source.index(after: colon)...]))
    }

    /// Changelog sections as the Markdown the app renders, newest first.
    static func markdown(_ entries: [ChangelogEntry]) -> String {
        entries.map { "## \($0.heading ?? $0.version)\n\n\($0.body)" }.joined(separator: "\n\n")
    }
}

/// Turns the core's events into the app's, remembering which package a task
/// id downloads for, because the app's progress is keyed by package.
struct ContractEventMapper {
    private var labels: [UInt64: String] = [:]

    mutating func map(_ event: ContractScenario.Event) -> [CoreEvent] {
        switch event.type {
        case "step":
            guard let package = event.package, let stage = Self.stage(event.stage) else { return [] }
            return [.step(package: package, stage: stage)]
        case "status", "success":
            return [.status([event.verb, event.detail].compactMap { $0 }.joined(separator: " "))]
        case "warn":
            return [.warning(event.detail ?? "")]
        case "note":
            return [.status(event.detail ?? "")]
        case "began":
            if let id = event.id, let label = event.task?.label { labels[id] = label }
            return []
        case "progress":
            guard let id = event.id, let label = labels[id] else { return [] }
            return [.progress(package: label, done: event.done ?? 0, total: event.total)]
        case "ended", "abandoned":
            // The app's events have no end of a task: the stage moving on is
            // what takes a bar down, and a dropped download is followed by the
            // error that stopped it.
            if let id = event.id { labels[id] = nil }
            return []
        default:
            return []
        }
    }

    /// The core's stages are finer than the app's: trusting is part of
    /// verifying, and installing is placing the links.
    private static func stage(_ name: String?) -> Stage? {
        switch name {
        case "resolving": .resolve
        case "downloading": .download
        case "verifying", "trusting": .verify
        case "extracting": .extract
        case "installing": .link
        default: nil
        }
    }
}

/// A JSON document of any shape, so a scenario's `value` can wait to be
/// decoded as whatever type its call returns.
private enum JSONValue: Codable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    init(from decoder: any Decoder) throws {
        let single = try decoder.singleValueContainer()
        if single.decodeNil() {
            self = .null
        } else if let value = try? single.decode(Bool.self) {
            self = .bool(value)
        } else if let value = try? single.decode(Double.self) {
            self = .number(value)
        } else if let value = try? single.decode(String.self) {
            self = .string(value)
        } else if let value = try? single.decode([JSONValue].self) {
            self = .array(value)
        } else {
            self = .object(try single.decode([String: JSONValue].self))
        }
    }

    func encode(to encoder: any Encoder) throws {
        var single = encoder.singleValueContainer()
        switch self {
        case .null: try single.encodeNil()
        case .bool(let value): try single.encode(value)
        case .number(let value): try single.encode(value)
        case .string(let value): try single.encode(value)
        case .array(let value): try single.encode(value)
        case .object(let value): try single.encode(value)
        }
    }
}
