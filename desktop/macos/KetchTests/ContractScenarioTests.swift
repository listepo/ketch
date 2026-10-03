// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// KetchStore and FakeKetchCore driven by the contract scenarios, the same
// files the Windows and Linux fakes read: what the real core sends is what
// the app's flows are tested against.

import Foundation
import Testing

@testable import Ketch

@MainActor
@Suite struct ContractScenarioTests {
    /// `desktop/contract/scenarios`, found from this file so the tests need no
    /// bundle resources.
    private static let directory = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .appending(path: "contract/scenarios", directoryHint: .isDirectory)

    private static func scenario(_ name: String) throws -> ContractScenario {
        let data = try Data(contentsOf: directory.appending(path: "\(name).json"))
        return try JSONDecoder().decode(ContractScenario.self, from: data)
    }

    private func makeStore(
        scripting names: [String], installed: [InstalledPackage] = []
    ) throws -> (KetchStore, FakeKetchCore) {
        let core = FakeKetchCore(installed: installed)
        for name in names { try core.script(Self.scenario(name)) }
        return (KetchStore(core: core, settings: AppSettings(defaults: scratchDefaults())), core)
    }

    @Test func everyScenarioFileDecodesAndTheFakeKnowsItsCall() throws {
        let all = try ContractScenario.load(from: Self.directory)
        #expect(all.count >= 15)
        for scenario in all {
            try FakeKetchCore().script(scenario)
        }
    }

    @Test func readsReturnTheRecordsTheCoreSent() async throws {
        let (store, _) = try makeStore(scripting: ["installed", "outdated", "doctor"])

        await store.refresh()
        await store.runDoctor()

        #expect(store.installed.map(\.name) == ["fd", "ripgrep"])
        #expect(store.installed.first { $0.name == "ripgrep" }?.repo == "BurntSushi/ripgrep")
        #expect(store.updates.map(\.name) == ["ripgrep"])
        #expect(store.pinned.map(\.name) == ["fd"])
        #expect(store.pinned.first?.heldBy == "/work/app/ketch.lock")
        #expect(store.problemCount == 2)
    }

    @Test func theChangelogRangeIsOneSectionPerRelease() async throws {
        let (store, _) = try makeStore(scripting: ["changelog-range"])

        let text = await store.changelog(for: "ripgrep", from: "14.1.0", to: nil)

        let plain = text.map { String($0.characters) } ?? ""
        #expect(plain.contains("Fix a regression in --json."))
        #expect(plain.contains("Performance improvements."))
    }

    @Test func aCleanInstallWalksEveryStageAndRecordsThePackage() async throws {
        let (store, _) = try makeStore(scripting: ["install-ok"])

        await store.install("ripgrep")

        #expect(store.errorMessage == nil)
        #expect(store.installed.map(\.name) == ["ripgrep"])
        #expect(store.installed.first?.version == "14.1.1")
        let messages = store.log.map(\.message)
        for stage in ["resolve", "download", "verify", "extract", "link"] {
            #expect(messages.contains("ripgrep: \(stage)"), "missing \(stage)")
        }
        #expect(messages.contains("installed ripgrep 14.1.1"))
    }

    @Test func aHeldLockOffersRetryWithTheHoldersPid() async throws {
        let (store, _) = try makeStore(scripting: ["install-busy"])

        await store.install("ripgrep")

        #expect(store.busy?.pid == 4242)
        #expect(store.errorMessage == nil)
        #expect(store.installed.isEmpty)
    }

    @Test func aLockWithNoKnownHolderStillOffersRetry() async throws {
        let (store, _) = try makeStore(scripting: ["install-busy-unknown-holder"])

        await store.install("ripgrep")

        #expect(store.busy != nil)
        #expect(store.errorMessage == nil)
    }

    @Test func cancelledIsLoggedNotShownAsAnAlert() async throws {
        let (store, _) = try makeStore(scripting: ["install-cancelled"])

        await store.install("ripgrep")

        #expect(store.errorMessage == nil)
        #expect(store.busy == nil)
        #expect(store.log.contains { $0.level == .warning && $0.message == "Cancelled" })
        #expect(store.installed.isEmpty)
    }

    @Test func aNetworkFailureShowsTheCoresWording() async throws {
        let (store, _) = try makeStore(scripting: ["install-network-failure"])

        await store.install("ripgrep")

        #expect(store.errorMessage?.hasPrefix("Network error: HTTP 503 from https://github.com/") == true)
        #expect(store.installed.isEmpty)
    }

    @Test func aVerificationFailureShowsTheCoresWording() async throws {
        let (store, _) = try makeStore(scripting: ["install-verification-failure"])

        await store.install("ripgrep")

        #expect(store.errorMessage?.hasPrefix("Verification failed: checksum mismatch") == true)
    }

    @Test func notFoundNamesThePackage() async throws {
        let (store, _) = try makeStore(scripting: ["install-not-found"])

        await store.install("nope")

        #expect(store.errorMessage == "No package named nope.")
    }

    @Test func aBinaryChoiceIsPutToTheSheetAndTheAnswerLinksThatBinary() async throws {
        let (store, core) = try makeStore(scripting: ["install-binary-choice"])

        let running = Task { await store.install("uv") }
        try await waitUntil { store.pendingChoice != nil }
        #expect(store.pendingChoice?.candidates == ["uv", "uvx"])
        store.answer(1)
        await running.value

        #expect(core.calls.contains("decision uv 1"))
        #expect(store.installed.map(\.name) == ["uv"])
        #expect(store.errorMessage == nil)
    }

    @Test func decliningTheBinaryChoiceStopsTheInstall() async throws {
        let (store, _) = try makeStore(scripting: ["install-binary-choice"])

        let running = Task { await store.install("uv") }
        try await waitUntil { store.pendingChoice != nil }
        store.answer(nil)
        await running.value

        #expect(store.installed.isEmpty)
    }

    @Test func anUpgradeReplacesTheInstalledVersion() async throws {
        let (store, _) = try makeStore(scripting: ["installed", "upgrade-stops-processes"])
        await store.refresh()

        await store.upgrade(["ripgrep"])

        #expect(store.errorMessage == nil)
        #expect(store.installed.first { $0.name == "ripgrep" }?.version == "14.1.1")
    }

    @Test func anUpgradeWithNothingToDoLeavesThingsAsTheyWere() async throws {
        let (store, _) = try makeStore(scripting: ["installed", "upgrade-nothing-to-do"])
        await store.refresh()

        await store.upgrade()

        #expect(store.installed.map(\.name) == ["fd", "ripgrep"])
        #expect(store.errorMessage == nil)
    }

    @Test func anUninstallRemovesTheRecord() async throws {
        let (store, _) = try makeStore(scripting: ["installed", "uninstall-ok"])
        await store.refresh()

        await store.uninstall(["ripgrep"])

        #expect(store.installed.map(\.name) == ["fd"])
    }

    @Test func uninstallingAnUnknownNameShowsTheError() async throws {
        let (store, _) = try makeStore(scripting: ["uninstall-not-found"])

        await store.uninstall(["nope"])

        #expect(store.errorMessage == "No package named nope.")
    }

    @Test func aCancelBetweenReplayedStepsStopsTheInstall() async throws {
        let core = FakeKetchCore(stepDelay: .milliseconds(30), installed: [])
        try core.script(Self.scenario("install-ok"))
        let store = KetchStore(core: core, settings: AppSettings(defaults: scratchDefaults()))

        let running = Task { await store.install("ripgrep") }
        try await waitUntil { store.activity != nil }
        store.cancel()
        await running.value

        #expect(store.installed.isEmpty)
        #expect(store.errorMessage == nil)
    }
}
