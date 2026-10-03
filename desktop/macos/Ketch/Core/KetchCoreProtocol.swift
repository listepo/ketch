// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The app's view of ketch's core: the operations, records, callbacks and
// errors R9's `ketch-ffi` will export through UniFFI.
//
// Views and the store only ever see this protocol, so they are testable
// against `FakeKetchCore`, and wiring the real core is a change to
// `CoreFactory.swift` plus one adapter file — nothing here moves.

import Foundation
import Synchronization

/// A package installed under the ketch root.
struct InstalledPackage: Sendable, Hashable, Identifiable {
    var name: String
    var version: String
    /// Where releases come from, e.g. `github` or a plugin name.
    var source: String
    /// `owner/repo` on GitHub, when the source has one.
    var repo: String?
    var description: String?
    /// The package's folder in the store, for "Reveal in Finder".
    var path: String

    var id: String { name }
}

/// A package the registry knows about.
struct RegistryPackage: Sendable, Hashable, Identifiable {
    var name: String
    var repo: String
    var description: String?
    var latest: String?

    var id: String { name }
}

/// An installed package with a newer release.
struct Upgrade: Sendable, Hashable, Identifiable {
    var name: String
    var from: String
    var to: String
    /// The `ketch.lock` that pins this package at `from`, when one does;
    /// `upgrade` leaves a held package where it is.
    var heldBy: String?

    var id: String { name }
}

/// One `ketch doctor` check.
struct Finding: Sendable, Hashable, Identifiable {
    enum Severity: Sendable, Hashable {
        case ok, warning, error
    }

    var id: String
    var severity: Severity
    var message: String
    /// The fix the core offers, as a label for a button; `nil` when there is none.
    var fix: String?
}

/// Options for `install`.
struct InstallOptions: Sendable, Hashable {
    var includePrereleases = false
    var force = false
}

/// A pipeline stage a package goes through, in order.
enum Stage: String, Sendable, Hashable, CaseIterable {
    case resolve, download, verify, extract, link, hooks
}

/// What the core reports while it works (R6's reporter events).
enum CoreEvent: Sendable, Hashable {
    case step(package: String, stage: Stage)
    case progress(package: String, done: UInt64, total: UInt64?)
    case status(String)
    case warning(String)
}

/// Receives events from a running operation, on the core's thread.
protocol Reporter: Sendable {
    func event(_ event: CoreEvent)
}

/// Answers the questions the core asks mid-operation (R7's decider), on the
/// core's thread. The core blocks until it returns.
protocol Decider: Sendable {
    /// Which of `candidates` to link for `package`; `nil` declines.
    func chooseBinary(package: String, candidates: [String]) -> Int?
}

/// Errors the core reports, mirroring R9's `KetchError`.
enum KetchError: Error, Sendable, Hashable {
    /// Another process holds the ketch lock.
    case busy(pid: UInt32)
    case cancelled
    case notFound(name: String)
    case network(message: String)
    case verification(message: String)
    case other(message: String)
}

extension KetchError: LocalizedError {
    var errorDescription: String? {
        switch self {
        case .busy(let pid): "ketch is running in another process (pid \(pid))."
        case .cancelled: "Cancelled."
        case .notFound(let name): "No package named \(name)."
        case .network(let message): "Network error: \(message)"
        case .verification(let message): "Verification failed: \(message)"
        case .other(let message): message
        }
    }
}

/// Cancels a running operation. The core polls `isCancelled` between steps;
/// `onCancel` lets an adapter forward the request to the FFI token.
final class CancelToken: Sendable {
    private let state = Mutex<(cancelled: Bool, handlers: [@Sendable () -> Void])>((false, []))

    init() {}

    var isCancelled: Bool { state.withLock { $0.cancelled } }

    func cancel() {
        let handlers = state.withLock { state -> [@Sendable () -> Void] in
            guard !state.cancelled else { return [] }
            state.cancelled = true
            defer { state.handlers = [] }
            return state.handlers
        }
        for handler in handlers { handler() }
    }

    /// Runs `handler` once when the token is cancelled, or now if it already is.
    func onCancel(_ handler: @escaping @Sendable () -> Void) {
        let runNow = state.withLock { state -> Bool in
            if state.cancelled { return true }
            state.handlers.append(handler)
            return false
        }
        if runNow { handler() }
    }
}

/// ketch's core as the app uses it. Every method is synchronous and may
/// block for a long time, so callers run it off the main actor.
protocol KetchCoreProtocol: Sendable {
    /// The ketch root this core manages.
    var root: URL { get }

    func installed() throws -> [InstalledPackage]
    func search(query: String) throws -> [RegistryPackage]
    func outdated() throws -> [Upgrade]
    func install(
        spec: String, options: InstallOptions,
        reporter: any Reporter, decider: any Decider, cancel: CancelToken
    ) throws
    /// Upgrades `names`, or everything outdated when `names` is empty.
    func upgrade(
        names: [String],
        reporter: any Reporter, decider: any Decider, cancel: CancelToken
    ) throws
    func uninstall(names: [String], reporter: any Reporter, cancel: CancelToken) throws
    /// The changelog between two versions, already sanitized by the core.
    func changelog(name: String, from: String?, to: String?) throws -> String
    func doctor() throws -> [Finding]
    /// The package a `ketch://package/<name>` link names. Everything in a link
    /// is untrusted, so the core validates it; a link that names anything but
    /// a package page throws.
    func packageName(forLink link: String) throws -> String
}
