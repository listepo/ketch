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
        /// The pid holding the ketch lock, or `nil` when it is free.
        var lockHolder: UInt32?
        /// Every operation called, in order, for tests to assert on.
        var calls: [String] = []
    }

    let root: URL
    private let stepDelay: Duration
    private let state: Mutex<State>

    init(
        root: URL = URL(fileURLWithPath: "/tmp/ketch-fake", isDirectory: true),
        stepDelay: Duration = .zero,
        installed: [InstalledPackage] = FakeKetchCore.sampleInstalled,
        registry: [RegistryPackage] = FakeKetchCore.sampleRegistry,
        binaries: [String: [String]] = ["uv": ["uv", "uvx"]]
    ) {
        self.root = root
        self.stepDelay = stepDelay
        state = Mutex(State(installed: installed, registry: registry, binaries: binaries))
    }

    // MARK: Test switches

    /// Simulates another ketch process holding the lock (`nil` releases it).
    func holdLock(pid: UInt32?) {
        state.withLock { $0.lockHolder = pid }
    }

    var calls: [String] { state.withLock { $0.calls } }

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
            return state.installed.compactMap { package in
                guard let latest = state.registry.first(where: { $0.name == package.name })?.latest,
                    latest != package.version
                else { return nil }
                return Upgrade(name: package.name, from: package.version, to: latest)
            }
        }
    }

    func install(
        spec: String, options: InstallOptions,
        reporter: any Reporter, decider: any Decider, cancel: CancelToken
    ) throws {
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
        try checkLock("upgrade \(names.joined(separator: " "))")
        let targets = try outdated().filter { names.isEmpty || names.contains($0.name) }
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
    ]

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
