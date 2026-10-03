// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// A stand-in for R9's core: canned packages, simulated pipeline stages and
// progress, and switches for the states the UI must handle (a held lock, a
// package with several binaries). Previews, tests and — until `ketch-ffi`
// exists — the app itself run on it. It never touches the disk.

import Foundation
import Synchronization

final class FakeKetchCore: KetchCoreProtocol {
    struct State: Sendable {
        var installed: [InstalledPackage]
        var registry: [RegistryPackage]
        /// Packages that ship several binaries, so linking asks the decider.
        var binaries: [String: [String]]
        /// Package name to the `ketch.lock` that pins it.
        var pins: [String: String]
        /// The pid holding the ketch lock, or `nil` when it is free.
        var lockHolder: UInt32?
        /// Every operation called, in order, for tests to assert on.
        var calls: [String] = []
        /// Contract scenarios to replay for install, upgrade and uninstall,
        /// by operation name.
        var scripted: [String: ContractScenario] = [:]
        /// Answers a contract scenario fixed for the reads, instead of the
        /// samples' own.
        var upgrades: [Upgrade]?
        var findings: [Finding]?
        var changelogText: String?
    }

    let root: URL
    private let stepDelay: Duration
    private let state: Mutex<State>

    init(
        root: URL = URL(fileURLWithPath: "/tmp/ketch-fake", isDirectory: true),
        stepDelay: Duration = .zero,
        installed: [InstalledPackage] = FakeKetchCore.sampleInstalled,
        registry: [RegistryPackage] = FakeKetchCore.sampleRegistry,
        binaries: [String: [String]] = ["uv": ["uv", "uvx"]],
        pins: [String: String] = FakeKetchCore.samplePins
    ) {
        self.root = root
        self.stepDelay = stepDelay
        state = Mutex(State(installed: installed, registry: registry, binaries: binaries, pins: pins))
    }

    // MARK: Test switches

    /// Simulates another ketch process holding the lock (`nil` releases it).
    func holdLock(pid: UInt32?) {
        state.withLock { $0.lockHolder = pid }
    }

    var calls: [String] { state.withLock { $0.calls } }

    /// Makes the fake answer as the core did in `scenario`: a read returns the
    /// scenario's records, and install, upgrade and uninstall replay its
    /// events, questions and outcome. Scenarios come from
    /// `desktop/contract/scenarios`, generated from `ketch-ffi`'s types.
    func script(_ scenario: ContractScenario) throws {
        switch scenario.call.operation {
        case "installed":
            let packages = try scenario.value(as: [ContractScenario.Package].self)
            state.withLock { $0.installed = packages.map(\.asInstalled) }
        case "search":
            let known = try scenario.value(as: ContractScenario.SearchResults.self).known
            state.withLock { $0.registry = known.map(\.asRegistryPackage) }
        case "outdated":
            let upgrades = try scenario.value(as: [ContractScenario.UpgradeEntry].self)
            state.withLock { $0.upgrades = upgrades.map(\.asUpgrade) }
        case "doctor":
            let checks = try scenario.value(as: [ContractScenario.CheckEntry].self)
            state.withLock { $0.findings = checks.map(\.asFinding) }
        case "changelog_range":
            let entries = try scenario.value(as: [ContractScenario.ChangelogEntry].self)
            state.withLock { $0.changelogText = ContractScenario.markdown(entries) }
        case "install", "upgrade", "uninstall":
            state.withLock { $0.scripted[scenario.call.operation] = scenario }
        case let other:
            throw KetchError.other(message: "no fake behaviour for the scenario operation \(other)")
        }
    }

    // MARK: KetchCoreProtocol

    func installed() throws -> [InstalledPackage] {
        state.withLock { state in
            state.calls.append("installed")
            return state.installed.sorted { $0.name < $1.name }
        }
    }

    func search(query: String) throws -> [RegistryPackage] {
        pause()
        let needle = query.trimmingCharacters(in: .whitespaces).lowercased()
        return state.withLock { state in
            state.calls.append("search \(needle)")
            guard !needle.isEmpty else { return state.registry }
            return state.registry.filter {
                $0.name.contains(needle) || ($0.description?.lowercased().contains(needle) ?? false)
            }
        }
    }

    func outdated() throws -> [Upgrade] {
        state.withLock { state in
            state.calls.append("outdated")
            if let upgrades = state.upgrades { return upgrades }
            return state.installed.compactMap { package in
                guard let latest = state.registry.first(where: { $0.name == package.name })?.latest,
                    latest != package.version
                else { return nil }
                return Upgrade(
                    name: package.name, from: package.version, to: latest, heldBy: state.pins[package.name])
            }
        }
    }

    func install(
        spec: String, options: InstallOptions,
        reporter: any Reporter, decider: any Decider, cancel: CancelToken
    ) throws {
        if let scenario = scripted("install") {
            state.withLock { $0.calls.append("install \(spec)") }
            return try replay(scenario, reporter: reporter, decider: decider, cancel: cancel)
        }
        let parts = spec.split(separator: "@", maxSplits: 1, omittingEmptySubsequences: false)
        let name = String(parts[0])
        let entry = try state.withLock { state -> RegistryPackage in
            state.calls.append("install \(spec)")
            if let pid = state.lockHolder { throw KetchError.busy(pid: pid) }
            guard let entry = state.registry.first(where: { $0.name == name }) else {
                throw KetchError.notFound(name: name)
            }
            return entry
        }
        let pinned = parts.count > 1 && !parts[1].isEmpty ? String(parts[1]) : nil
        let version = pinned ?? entry.latest ?? "0.0.0"
        try runPipeline(name, reporter: reporter, decider: decider, cancel: cancel)
        state.withLock { state in
            state.installed.removeAll { $0.name == name }
            state.installed.append(
                InstalledPackage(
                    name: name, version: version, source: "github", repo: entry.repo,
                    description: entry.description, path: root.appending(path: "store/\(name)").path))
        }
        reporter.event(.status("Installed \(name) \(version)"))
    }

    func upgrade(
        names: [String],
        reporter: any Reporter, decider: any Decider, cancel: CancelToken
    ) throws {
        if let scenario = scripted("upgrade") {
            state.withLock { $0.calls.append("upgrade \(names.joined(separator: " "))") }
            return try replay(scenario, reporter: reporter, decider: decider, cancel: cancel)
        }
        try checkLock("upgrade \(names.joined(separator: " "))")
        let targets = try outdated().filter { $0.heldBy == nil && (names.isEmpty || names.contains($0.name)) }
        if targets.isEmpty { reporter.event(.status("Everything is up to date")) }
        for target in targets {
            try runPipeline(target.name, reporter: reporter, decider: decider, cancel: cancel)
            state.withLock { state in
                if let index = state.installed.firstIndex(where: { $0.name == target.name }) {
                    state.installed[index].version = target.to
                }
            }
            reporter.event(.status("Upgraded \(target.name) \(target.from) → \(target.to)"))
        }
    }

    func uninstall(names: [String], reporter: any Reporter, cancel: CancelToken) throws {
        if let scenario = scripted("uninstall") {
            state.withLock { $0.calls.append("uninstall \(names.joined(separator: " "))") }
            return try replay(scenario, reporter: reporter, decider: NoDecider(), cancel: cancel)
        }
        try checkLock("uninstall \(names.joined(separator: " "))")
        for name in names {
            if cancel.isCancelled { throw KetchError.cancelled }
            pause()
            try state.withLock { state in
                guard state.installed.contains(where: { $0.name == name }) else {
                    throw KetchError.notFound(name: name)
                }
                state.installed.removeAll { $0.name == name }
            }
            reporter.event(.status("Uninstalled \(name)"))
        }
    }

    func changelog(name: String, from: String?, to: String?) throws -> String {
        pause()
        state.withLock { $0.calls.append("changelog \(name)") }
        if let text = state.withLock({ $0.changelogText }) { return text }
        let to = to ?? "latest"
        return """
            ## \(to)

            ### Features

            - **Faster search** across large trees.
            - New `--json` output, see [the docs](https://github.com/pyrlyn/ketch).

            ### Fixes

            - Handles paths with spaces.

            ## \(from ?? "previous")

            - Initial release notes for \(name).
            """
    }

    func doctor() throws -> [Finding] {
        pause()
        state.withLock { $0.calls.append("doctor") }
        if let findings = state.withLock({ $0.findings }) { return findings }
        return [
            Finding(id: "root", severity: .ok, message: "ketch root is \(root.path)", fix: nil),
            Finding(id: "path", severity: .warning, message: "The bin dir is not on PATH in zsh", fix: "Add to PATH"),
            Finding(id: "links", severity: .ok, message: "All links resolve", fix: nil),
        ]
    }

    // MARK: Simulation

    private func checkLock(_ call: String) throws {
        try state.withLock { state in
            state.calls.append(call)
            if let pid = state.lockHolder { throw KetchError.busy(pid: pid) }
        }
    }

    private func runPipeline(
        _ name: String, reporter: any Reporter, decider: any Decider, cancel: CancelToken
    ) throws {
        let total: UInt64 = 4_000_000
        for stage in Stage.allCases {
            if cancel.isCancelled { throw KetchError.cancelled }
            reporter.event(.step(package: name, stage: stage))
            switch stage {
            case .download:
                for chunk in 0...4 {
                    if cancel.isCancelled { throw KetchError.cancelled }
                    reporter.event(.progress(package: name, done: total / 4 * UInt64(chunk), total: total))
                    pause()
                }
            case .link:
                let candidates = state.withLock { $0.binaries[name] ?? [] }
                if candidates.count > 1 {
                    guard let choice = decider.chooseBinary(package: name, candidates: candidates),
                        candidates.indices.contains(choice)
                    else { throw KetchError.cancelled }
                    reporter.event(.status("Linked \(candidates[choice]) for \(name)"))
                }
                pause()
            default:
                pause()
            }
        }
    }

    private func scripted(_ operation: String) -> ContractScenario? {
        state.withLock { $0.scripted[operation] }
    }

    /// Plays a contract scenario: its events to the reporter, its questions to
    /// the decider (which answers, not the file), then its outcome. A cancel
    /// is honoured between steps, as the real pipeline does.
    private func replay(
        _ scenario: ContractScenario,
        reporter: any Reporter, decider: any Decider, cancel: CancelToken
    ) throws {
        var mapper = ContractEventMapper()
        for step in scenario.script {
            if cancel.isCancelled { throw KetchError.cancelled }
            switch step {
            case .event(let event):
                for mapped in mapper.map(event) { reporter.event(mapped) }
            case .ask(let question):
                // `stop_processes` has no counterpart in the app's decider yet
                // (the live adapter adds it); only the binary choice is asked.
                guard question.type == "choose_binary", let package = question.package,
                    let candidates = question.candidates
                else { continue }
                let pick = decider.chooseBinary(package: package, candidates: candidates)
                state.withLock { $0.calls.append("decision \(package) \(pick.map(String.init) ?? "none")") }
                guard let pick, candidates.indices.contains(pick) else { throw KetchError.cancelled }
            }
            pause()
        }
        switch scenario.call.operation {
        case "uninstall":
            let removed = try scenario.value(as: [ContractScenario.Package].self).map(\.name)
            state.withLock { state in state.installed.removeAll { removed.contains($0.name) } }
        default:
            let placed = try scenario.value(as: [ContractScenario.Installed].self).map(\.package.asInstalled)
            state.withLock { state in
                for package in placed {
                    state.installed.removeAll { $0.name == package.name }
                    state.installed.append(package)
                }
            }
        }
    }

    private func pause() {
        guard stepDelay > .zero else { return }
        let (seconds, attoseconds) = stepDelay.components
        Thread.sleep(forTimeInterval: Double(seconds) + Double(attoseconds) / 1e18)
    }
}

extension FakeKetchCore {
    static let sampleInstalled: [InstalledPackage] = [
        InstalledPackage(
            name: "ripgrep", version: "14.1.0", source: "github", repo: "BurntSushi/ripgrep",
            description: "Recursively search directories for a regex pattern",
            path: "/tmp/ketch-fake/store/ripgrep"),
        InstalledPackage(
            name: "fd", version: "10.2.0", source: "github", repo: "sharkdp/fd",
            description: "A simple, fast and user-friendly alternative to find",
            path: "/tmp/ketch-fake/store/fd"),
        InstalledPackage(
            name: "bat", version: "0.24.0", source: "github", repo: "sharkdp/bat",
            description: "A cat clone with wings", path: "/tmp/ketch-fake/store/bat"),
        InstalledPackage(
            name: "jq", version: "1.7.1", source: "github", repo: "jqlang/jq",
            description: "Command-line JSON processor", path: "/tmp/ketch-fake/store/jq"),
    ]

    /// jq is held back by a project's lockfile, so the Updates screen has a
    /// pinned package to show.
    static let samplePins: [String: String] = ["jq": "~/work/site/ketch.lock"]

    static let sampleRegistry: [RegistryPackage] = [
        RegistryPackage(
            name: "ripgrep", repo: "BurntSushi/ripgrep",
            description: "Recursively search directories for a regex pattern", latest: "14.1.1"),
        RegistryPackage(
            name: "fd", repo: "sharkdp/fd",
            description: "A simple, fast and user-friendly alternative to find", latest: "10.2.0"),
        RegistryPackage(name: "bat", repo: "sharkdp/bat", description: "A cat clone with wings", latest: "0.25.0"),
        RegistryPackage(name: "jq", repo: "jqlang/jq", description: "Command-line JSON processor", latest: "1.8.1"),
        RegistryPackage(
            name: "uv", repo: "astral-sh/uv", description: "An extremely fast Python package manager",
            latest: "0.9.0"),
        RegistryPackage(
            name: "zoxide", repo: "ajeetdsouza/zoxide", description: "A smarter cd command", latest: "0.9.8"),
    ]
}

/// For an operation that never asks.
private struct NoDecider: Decider {
    func chooseBinary(package: String, candidates: [String]) -> Int? { nil }
}
