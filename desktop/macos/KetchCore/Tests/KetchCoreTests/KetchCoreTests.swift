// The real core, through the generated bindings, against a scratch ketch
// root: what proves the XCFramework links, the bindings match the library,
// and callbacks cross the boundary in both directions.

import Foundation
import KetchCore
import Synchronization
import Testing

/// Keeps every event the core reports, from whichever thread reports it.
final class Recorder: Reporter {
    private let events = Mutex<[Event]>([])

    func event(event: Event) {
        events.withLock { $0.append(event) }
    }

    var received: [Event] { events.withLock { $0 } }
}

/// A throwaway directory holding a ketch root and a one-file package to
/// install from it.
struct Scratch: ~Copyable {
    let dir: URL
    var root: String { dir.appending(path: "root").path(percentEncoded: false) }
    let tool: URL

    init() throws {
        dir = FileManager.default.temporaryDirectory
            .appending(path: "ketch-core-tests-\(UUID().uuidString)")
        tool = dir.appending(path: "payload/hello")
        try FileManager.default.createDirectory(
            at: tool.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("#!/bin/sh\necho hello\n".utf8).write(to: tool)
    }

    deinit { try? FileManager.default.removeItem(at: dir) }
}

@Test func theLibraryReportsItsVersion() {
    #expect(!ketchVersion().isEmpty)
}

@Test func aPackageLinkYieldsItsNameAndAnInstallLinkIsRefused() throws {
    #expect(try packageForLink(url: "ketch://package/ripgrep") == "ripgrep")
    #expect(throws: KetchError.self) { try packageForLink(url: "ketch://install/ripgrep") }
}

@Test func aLocalPackageInstallsIntoAScratchRootAndReportsItsStages() throws {
    let scratch = try Scratch()
    let recorder = Recorder()
    let core = KetchCore(root: scratch.root, reporter: recorder, decider: nil)

    #expect(try core.installed().isEmpty)
    let placed = try core.install(
        specs: ["local:\(scratch.tool.path(percentEncoded: false))"],
        options: InstallOptions(), cancel: nil)

    #expect(placed.count == 1)
    let name = try #require(placed.first).package.name
    #expect(try core.installed().map(\.name) == [name])
    #expect(recorder.received.contains(.step(package: name, stage: .installing)))

    let removed = try core.uninstall(names: [name])
    #expect(removed.map(\.name) == [name])
    #expect(try core.installed().isEmpty)
}

@Test func aCancelledInstallThrowsCancelledAndPlacesNothing() throws {
    let scratch = try Scratch()
    let core = KetchCore(root: scratch.root, reporter: nil, decider: nil)
    let token = CancelToken()
    token.cancel()

    #expect(throws: KetchError.Cancelled) {
        try core.install(
            specs: ["local:\(scratch.tool.path(percentEncoded: false))"],
            options: InstallOptions(), cancel: token)
    }
    #expect(try core.installed().isEmpty)
}

@Test func anUnknownPackageIsNotFound() throws {
    let scratch = try Scratch()
    let core = KetchCore(root: scratch.root, reporter: nil, decider: nil)

    #expect(throws: KetchError.NotFound(name: "nope")) {
        try core.uninstall(names: ["nope"])
    }
}
