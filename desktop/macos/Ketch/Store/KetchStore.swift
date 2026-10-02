// The app's one source of state. It lives on the main actor; every core call
// runs on a background queue, and the core's reporter and decider callbacks
// hop back here, so views never block and never see a half-applied event.

import Foundation
import Observation
import SwiftUI
import Synchronization

/// The operation currently running, as the Activity pane and menu bar show it.
struct Activity: Sendable, Hashable {
    struct PackageProgress: Sendable, Hashable {
        var stage: Stage
        var done: UInt64 = 0
        var total: UInt64?

        /// 0...1 across all stages, with the download filling its own share.
        var fraction: Double {
            let index = Double(Stage.allCases.firstIndex(of: stage) ?? 0)
            let count = Double(Stage.allCases.count)
            var within = 0.0
            if stage == .download, let total, total > 0 { within = Double(done) / Double(total) }
            return min(1, (index + within) / count)
        }
    }

    var title: String
    var packages: [String: PackageProgress] = [:]
    var status: String?
    var isCancelling = false

    /// Mean progress over the packages seen so far.
    var fraction: Double {
        guard !packages.isEmpty else { return 0 }
        return packages.values.map(\.fraction).reduce(0, +) / Double(packages.count)
    }
}

/// One line of the Activity log.
struct LogEntry: Sendable, Hashable, Identifiable {
    enum Level: Sendable, Hashable { case info, success, warning, error }

    let id: Int
    let date: Date
    let level: Level
    let message: String
}

/// A binary-choice question waiting for the user.
struct BinaryChoice: Identifiable {
    let id = UUID()
    let package: String
    let candidates: [String]
    fileprivate let answer: @Sendable (Int?) -> Void
}

/// Another process holds the ketch lock; `retry` re-runs what was refused.
struct BusyState: Identifiable {
    let id = UUID()
    let pid: UInt32
    let retry: @MainActor () async -> Void
}

@MainActor
@Observable
final class KetchStore {
    let core: any KetchCoreProtocol
    let settings: AppSettings

    private(set) var installed: [InstalledPackage] = []
    private(set) var outdated: [Upgrade] = []
    private(set) var searchResults: [RegistryPackage] = []
    private(set) var findings: [Finding] = []
    private(set) var activity: Activity?
    private(set) var log: [LogEntry] = []
    private(set) var lastChecked: Date?

    /// Set when the core refused because another process holds the lock.
    var busy: BusyState?
    /// An error to show once; the view clears it.
    var errorMessage: String?
    /// A question from the core's decider, shown as a sheet.
    var pendingChoice: BinaryChoice?
    /// A package a `ketch://` link asked to show; Discover consumes it.
    var linkedPackage: String?
    /// Upgrade-all waits for this confirmation, which the menu bar can raise.
    var confirmingUpgradeAll = false

    @ObservationIgnored private var cancelToken: CancelToken?
    @ObservationIgnored private var updateLoop: Task<Void, Never>?
    @ObservationIgnored private var nextLogID = 0

    init(core: any KetchCoreProtocol, settings: AppSettings) {
        self.core = core
        self.settings = settings
    }

    var root: URL { core.root }
    var isRunning: Bool { activity != nil }

    /// Newer releases `upgrade` would install: everything outdated that no
    /// `ketch.lock` holds back.
    var updates: [Upgrade] { outdated.filter { $0.heldBy == nil } }
    /// Newer releases a `ketch.lock` holds back, shown but never upgraded.
    var pinned: [Upgrade] { outdated.filter { $0.heldBy != nil } }
    /// What the sidebar, the menu bar and Upgrade all count.
    var pendingUpgradeCount: Int { updates.count }
    /// Doctor findings that are not ok, for the sidebar.
    var problemCount: Int { findings.count { $0.severity != .ok } }

    /// The version an upgrade would move `name` to; `nil` when it is current
    /// or pinned.
    func outdatedVersion(of name: String) -> String? {
        updates.first { $0.name == name }?.to
    }

    // MARK: Reading

    /// Re-reads installed and outdated packages; called on launch, when the
    /// window becomes key (so CLI installs show up) and after every operation.
    func refresh() async {
        do {
            let (installed, outdated) = try await background { core in
                (try core.installed(), try core.outdated())
            }
            self.installed = installed
            self.outdated = outdated
            lastChecked = Date()
        } catch {
            report(error, retry: { [weak self] in await self?.refresh() })
        }
    }

    func search(_ query: String) async {
        do {
            searchResults = try await background { try $0.search(query: query) }
        } catch {
            report(error, retry: { [weak self] in await self?.search(query) })
        }
    }

    func changelog(for name: String, from: String?, to: String?) async -> AttributedString? {
        do {
            let markdown = try await background { try $0.changelog(name: name, from: from, to: to) }
            return Self.render(markdown: markdown)
        } catch {
            report(error, retry: nil)
            return nil
        }
    }

    /// Handles a `ketch://` link. It only ever shows a package page: nothing a
    /// link says installs, upgrades or removes anything. The core validates the
    /// link, and a refusal or an unknown package is shown like any other error.
    func open(link: URL) async {
        let text = link.absoluteString
        do {
            let known = installed.map(\.name)
            linkedPackage = try await background { core in
                let name = try core.packageName(forLink: text)
                let results = known.contains(name) ? [] : try core.search(query: name)
                guard known.contains(name) || results.contains(where: { $0.name == name })
                else { throw KetchError.notFound(name: name) }
                return name
            }
        } catch {
            report(error, retry: nil)
        }
    }

    func runDoctor() async {
        do {
            findings = try await background { try $0.doctor() }
        } catch {
            report(error, retry: { [weak self] in await self?.runDoctor() })
        }
    }

    /// Changelog Markdown as styled text. The core has already stripped
    /// escape sequences and bidi overrides; this only parses it. Headings are
    /// styled line by line because SwiftUI's `Text` drops the block structure
    /// a full Markdown parse produces.
    nonisolated static func render(markdown: String) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(
            interpretedSyntax: .inlineOnlyPreservingWhitespace, failurePolicy: .returnPartiallyParsedIfPossible)
        var result = AttributedString()
        for (index, line) in markdown.split(separator: "\n", omittingEmptySubsequences: false).enumerated() {
            if index > 0 { result += AttributedString("\n") }
            let body = line.drop { $0 == "#" }
            let level = line.count - body.count
            let isHeading = level > 0 && body.first == " "
            let source = String(isHeading ? body.drop { $0 == " " } : line)
            var text = (try? AttributedString(markdown: source, options: options)) ?? AttributedString(source)
            if isHeading { text.font = level <= 2 ? .title3.bold() : .headline }
            result += text
        }
        return result
    }

    // MARK: Operations

    func install(_ spec: String) async {
        let options = InstallOptions(includePrereleases: settings.includePrereleases)
        await operate(
            "Installing \(spec)",
            retry: { [weak self] in await self?.install(spec) },
            body: { core, reporter, decider, cancel in
                try core.install(spec: spec, options: options, reporter: reporter, decider: decider, cancel: cancel)
            })
    }

    /// Upgrades `names`, or everything outdated when empty.
    func upgrade(_ names: [String] = []) async {
        let title = names.isEmpty ? "Upgrading all packages" : "Upgrading \(names.joined(separator: ", "))"
        await operate(
            title,
            retry: { [weak self] in await self?.upgrade(names) },
            body: { core, reporter, decider, cancel in
                try core.upgrade(names: names, reporter: reporter, decider: decider, cancel: cancel)
            })
    }

    func uninstall(_ names: [String]) async {
        await operate(
            "Uninstalling \(names.joined(separator: ", "))",
            retry: { [weak self] in await self?.uninstall(names) },
            body: { core, reporter, _, cancel in
                try core.uninstall(names: names, reporter: reporter, cancel: cancel)
            })
    }

    /// Asks the running operation to stop at its next step, and declines any
    /// open question so the core is not left waiting on it.
    func cancel() {
        guard let cancelToken else { return }
        activity?.isCancelling = true
        cancelToken.cancel()
        answer(nil)
    }

    /// Re-runs what the held lock refused. The banner goes away now and comes
    /// back with the new holder's pid if the lock is still held.
    func retryBusy() async {
        guard let busy else { return }
        self.busy = nil
        await busy.retry()
    }

    /// Answers the pending binary choice.
    func answer(_ choice: Int?) {
        guard let pending = pendingChoice else { return }
        pendingChoice = nil
        pending.answer(choice)
    }

    // MARK: Update checks

    /// Checks now, then every `settings.updateCheckInterval` until stopped.
    /// Calling it again restarts the timer, e.g. after the interval changes.
    func startUpdateChecks() {
        updateLoop?.cancel()
        updateLoop = Task { [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                if !self.isRunning { await self.refresh() }
                let interval = self.settings.updateCheckInterval
                try? await Task.sleep(for: interval)
            }
        }
    }

    func stopUpdateChecks() {
        updateLoop?.cancel()
        updateLoop = nil
    }

    // MARK: Plumbing

    private func operate(
        _ title: String,
        retry: @escaping @MainActor () async -> Void,
        body: @escaping @Sendable (any KetchCoreProtocol, any Reporter, any Decider, CancelToken) throws -> Void
    ) async {
        guard !isRunning else {
            errorMessage = "Another operation is running. Wait for it or cancel it first."
            return
        }
        let token = CancelToken()
        cancelToken = token
        activity = Activity(title: title)
        busy = nil
        append(.info, title)
        let reporter = MainActorReporter(store: self)
        let decider = SheetDecider(store: self, cancel: token)
        do {
            try await background { core in try body(core, reporter, decider, token) }
            append(.success, "Done: \(title)")
        } catch {
            report(error, retry: retry)
        }
        cancelToken = nil
        activity = nil
        pendingChoice = nil
        await refresh()
    }

    /// Runs a blocking core call on a global queue — not in the cooperative
    /// pool, which a call that waits on the network or a decider would starve.
    private func background<T: Sendable>(
        _ work: @escaping @Sendable (any KetchCoreProtocol) throws -> T
    ) async throws -> T {
        let core = self.core
        return try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                continuation.resume(with: Result { try work(core) })
            }
        }
    }

    private func report(_ error: any Error, retry: (@MainActor () async -> Void)?) {
        switch error as? KetchError {
        case .busy(let pid)?:
            append(.warning, "ketch is running in another process (pid \(pid))")
            if let retry { busy = BusyState(pid: pid, retry: retry) }
        case .cancelled?:
            append(.warning, "Cancelled")
        default:
            let message = error.localizedDescription
            append(.error, message)
            errorMessage = message
        }
    }

    fileprivate func apply(_ event: CoreEvent) {
        switch event {
        case .step(let package, let stage):
            activity?.packages[package, default: .init(stage: stage)].stage = stage
            append(.info, "\(package): \(stage.rawValue)")
        case .progress(let package, let done, let total):
            activity?.packages[package, default: .init(stage: .download)].done = done
            activity?.packages[package]?.total = total
        case .status(let message):
            activity?.status = message
            append(.info, message)
        case .warning(let message):
            append(.warning, message)
        }
    }

    fileprivate func ask(package: String, candidates: [String], answer: @escaping @Sendable (Int?) -> Void) {
        guard cancelToken?.isCancelled == false else { return answer(nil) }
        pendingChoice?.answer(nil)
        pendingChoice = BinaryChoice(package: package, candidates: candidates, answer: answer)
    }

    private func append(_ level: LogEntry.Level, _ message: String) {
        nextLogID += 1
        log.append(LogEntry(id: nextLogID, date: Date(), level: level, message: message))
        if log.count > 500 { log.removeFirst(log.count - 500) }
    }
}

/// Forwards core events to the store in the order the core sent them:
/// `DispatchQueue.main` is FIFO, where separate `Task`s are not.
private struct MainActorReporter: Reporter {
    weak var store: KetchStore?

    func event(_ event: CoreEvent) {
        DispatchQueue.main.async { [weak store] in
            MainActor.assumeIsolated { store?.apply(event) }
        }
    }
}

/// Turns the core's blocking `chooseBinary` into a sheet: the core's thread
/// waits on a semaphore while the main actor shows the question.
private final class SheetDecider: Decider {
    private weak let store: KetchStore?
    private let cancel: CancelToken

    init(store: KetchStore, cancel: CancelToken) {
        self.store = store
        self.cancel = cancel
    }

    func chooseBinary(package: String, candidates: [String]) -> Int? {
        // Blocking the main thread on a question only it can answer would hang the app.
        guard !Thread.isMainThread, !cancel.isCancelled else { return nil }
        let answer = Mutex<Int?>(nil)
        let done = DispatchSemaphore(value: 0)
        DispatchQueue.main.async { [weak store] in
            MainActor.assumeIsolated {
                guard let store else {
                    done.signal()
                    return
                }
                store.ask(package: package, candidates: candidates) { choice in
                    answer.withLock { $0 = choice }
                    done.signal()
                }
            }
        }
        done.wait()
        return answer.withLock { $0 }
    }
}
