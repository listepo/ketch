// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

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

/// Declines every question and remembers the holders it was shown.
final class Declining: Decider {
    private let shown = Mutex<[Holder]>([])

    func chooseBinary(package: String, candidates: [String]) -> UInt32? { nil }

    func stopProcesses(holders: [Holder]) -> Bool {
        shown.withLock { $0.append(contentsOf: holders) }
        return false
    }

    var holders: [Holder] { shown.withLock { $0 } }
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

    /// Serve `sleeper` from a `test:` source plugin in this root, one release
    /// per version, each a tarball holding a script that loops until stopped
    /// — a program that can be left running while an upgrade replaces it.
    /// Not a copy of `/bin/sleep`: macOS kills a relocated platform binary.
    func publishSleeper(_ versions: [String]) throws {
        let assets = dir.appending(path: "assets")
        let plugins = dir.appending(path: "root/plugins")
        try FileManager.default.createDirectory(at: assets, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: plugins, withIntermediateDirectories: true)

        let plugin = plugins.appending(path: "ketch-source-test")
        let script = """
            #!/bin/sh
            set -eu
            case "$1" in
            capabilities) printf '%s' '{"protocol":1,"scheme":"test","download":true,"search":false}' ;;
            describe) printf 'null' ;;
            releases) cat '\(assets.path(percentEncoded: false))/'"$2.releases.json" ;;
            search) printf '[]' ;;
            download) cp "$2" "$3" ;;
            *) exit 1 ;;
            esac

            """
        try Data(script.utf8).write(to: plugin)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755], ofItemAtPath: plugin.path(percentEncoded: false))

        #if arch(arm64)
            let arch = "aarch64"
        #else
            let arch = "x86_64"
        #endif
        var releases: [String] = []
        for version in versions {
            let tarball = assets.appending(path: "sleeper-\(version)-\(arch)-apple-darwin.tar.gz")
            if !FileManager.default.fileExists(atPath: tarball.path(percentEncoded: false)) {
                let stage = dir.appending(path: "stage-\(version)")
                let top = stage.appending(path: "sleeper-\(version)")
                try FileManager.default.createDirectory(at: top, withIntermediateDirectories: true)
                let sleeper = top.appending(path: "sleeper")
                try Data("#!/bin/sh\nwhile :; do sleep 0.1; done\n".utf8).write(to: sleeper)
                try FileManager.default.setAttributes(
                    [.posixPermissions: 0o755], ofItemAtPath: sleeper.path(percentEncoded: false))
                try run(
                    "/usr/bin/tar", "-czf", tarball.path(percentEncoded: false),
                    "-C", stage.path(percentEncoded: false), "sleeper-\(version)")
            }
            releases.append(
                #"{"version":"\#(version)","tag":"v\#(version)","prerelease":false,"draft":false,"#
                    + #""assets":[{"name":"\#(tarball.lastPathComponent)","url":"\#(tarball.path(percentEncoded: false))"}]}"#
            )
        }
        try Data("[\(releases.joined(separator: ","))]".utf8)
            .write(to: assets.appending(path: "sleeper.releases.json"))
    }
}

/// Run a program to completion; a failure is a thrown error.
@discardableResult
func run(_ program: String, _ arguments: String...) throws -> String {
    let process = Process()
    process.executableURL = URL(filePath: program)
    process.arguments = arguments
    let out = Pipe()
    process.standardOutput = out
    process.standardError = FileHandle.nullDevice
    try process.run()
    let data = out.fileHandleForReading.readDataToEndOfFile()
    process.waitUntilExit()
    guard process.terminationStatus == 0 else {
        throw CocoaError(.executableLoad)
    }
    return String(decoding: data, as: UTF8.self)
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
    let core = KetchCore(root: scratch.root)

    #expect(try core.installed().isEmpty)
    let placed = try core.install(
        specs: ["local:\(scratch.tool.path(percentEncoded: false))"],
        options: InstallOptions(), reporter: recorder, decider: nil, cancel: nil)

    #expect(placed.count == 1)
    let name = try #require(placed.first).package.name
    #expect(try core.installed().map(\.name) == [name])
    #expect(recorder.received.contains(.step(package: name, stage: .installing)))

    let removed = try core.uninstall(names: [name], reporter: nil, decider: nil, cancel: nil)
    #expect(removed.map(\.name) == [name])
    #expect(try core.installed().isEmpty)
}

@Test func eachCallReportsToTheReporterItWasGiven() throws {
    let scratch = try Scratch()
    let core = KetchCore(root: scratch.root)
    let installing = Recorder()
    let removing = Recorder()

    let placed = try core.install(
        specs: ["local:\(scratch.tool.path(percentEncoded: false))"],
        options: InstallOptions(), reporter: installing, decider: nil, cancel: nil)
    let name = try #require(placed.first).package.name
    try core.uninstall(names: [name], reporter: removing, decider: nil, cancel: nil)

    #expect(installing.received.contains(.step(package: name, stage: .installing)))
    #expect(!removing.received.contains(.step(package: name, stage: .installing)))
}

@Test func anUpgradeAsksTheDeciderAboutARunningBinaryAndADeclineLeavesItRunning() throws {
    let scratch = try Scratch()
    let core = KetchCore(root: scratch.root)
    try scratch.publishSleeper(["1.0.0"])
    let placed = try core.install(
        specs: ["test:sleeper"], options: InstallOptions(),
        reporter: nil, decider: nil, cancel: nil)
    let link = try #require(placed.first?.package.binaries.first)

    let sleeper = Process()
    sleeper.executableURL = URL(filePath: link)
    try sleeper.run()
    defer { sleeper.terminate() }
    // The core finds holders with `lsof`, so wait until `lsof` does too
    // rather than for a guessed interval.
    let pid = "\(sleeper.processIdentifier)"
    let deadline = Date.now.addingTimeInterval(10)
    while (try? run("/usr/sbin/lsof", "-t", "--", link))?.contains(pid) != true {
        try #require(Date.now < deadline, "lsof never saw the running sleeper")
        Thread.sleep(forTimeInterval: 0.05)
    }

    try scratch.publishSleeper(["1.0.0", "1.1.0"])
    let decider = Declining()
    let upgraded = try core.upgrade(names: [], reporter: nil, decider: decider, cancel: nil)

    #expect(upgraded.map(\.package.version) == ["1.1.0"])
    #expect(decider.holders.map(\.pid) == [UInt32(sleeper.processIdentifier)])
    #expect(sleeper.isRunning)
}

@Test func aCancelledInstallThrowsCancelledAndPlacesNothing() throws {
    let scratch = try Scratch()
    let core = KetchCore(root: scratch.root)
    let token = CancelToken()
    token.cancel()

    #expect(throws: KetchError.Cancelled) {
        try core.install(
            specs: ["local:\(scratch.tool.path(percentEncoded: false))"],
            options: InstallOptions(), reporter: nil, decider: nil, cancel: token)
    }
    #expect(try core.installed().isEmpty)
}

@Test func anUnknownPackageIsNotFound() throws {
    let scratch = try Scratch()
    let core = KetchCore(root: scratch.root)

    #expect(throws: KetchError.NotFound(name: "nope")) {
        try core.uninstall(names: ["nope"], reporter: nil, decider: nil, cancel: nil)
    }
}
