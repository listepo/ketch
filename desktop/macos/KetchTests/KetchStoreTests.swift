// KetchStore against FakeKetchCore: the flows the UI depends on, without a
// window. Each test builds its own fake, so they run in any order.

import Foundation
import Testing

@testable import Ketch

@MainActor
@Suite struct KetchStoreTests {
    private func makeStore(
        _ core: FakeKetchCore = FakeKetchCore(), defaults: UserDefaults = scratchDefaults()
    ) -> KetchStore {
        KetchStore(core: core, settings: AppSettings(defaults: defaults))
    }

    @Test func installAddsThePackageAndLogsEveryStage() async {
        let core = FakeKetchCore(installed: [])
        let store = makeStore(core)

        await store.install("jq")

        #expect(store.installed.map(\.name) == ["jq"])
        #expect(store.installed.first?.version == "1.8.1")
        #expect(store.activity == nil)
        #expect(store.errorMessage == nil)
        let messages = store.log.map(\.message)
        for stage in Stage.allCases {
            #expect(messages.contains("jq: \(stage.rawValue)"))
        }
        #expect(store.log.contains { $0.level == .success && $0.message == "Done: Installing jq" })
    }

    @Test func installOfAnUnknownPackageShowsTheError() async {
        let store = makeStore()

        await store.install("nope")

        #expect(store.errorMessage == "No package named nope.")
        #expect(store.busy == nil)
    }

    @Test func busyLockOffersARetryThatSucceedsOnceTheLockIsFree() async throws {
        let core = FakeKetchCore(installed: [])
        let store = makeStore(core)
        core.holdLock(pid: 4242)

        await store.install("jq")

        let busy = try #require(store.busy)
        #expect(busy.pid == 4242)
        #expect(store.installed.isEmpty)
        #expect(store.errorMessage == nil)

        core.holdLock(pid: nil)
        await busy.retry()

        #expect(store.busy == nil)
        #expect(store.installed.map(\.name) == ["jq"])
    }

    @Test func cancelStopsTheOperationAtTheNextStep() async throws {
        let core = FakeKetchCore(stepDelay: .milliseconds(30), installed: [])
        let store = makeStore(core)

        let running = Task { await store.install("jq") }
        try await waitUntil { store.activity?.packages["jq"] != nil }
        store.cancel()
        await running.value

        #expect(store.installed.isEmpty)
        #expect(store.activity == nil)
        #expect(store.errorMessage == nil)
        #expect(store.log.last { $0.level == .warning }?.message == "Cancelled")
    }

    @Test func binaryChoiceIsAskedThroughTheSheetAndTheAnswerIsUsed() async throws {
        let store = makeStore(FakeKetchCore(installed: []))

        let running = Task { await store.install("uv") }
        try await waitUntil { store.pendingChoice != nil }
        let choice = try #require(store.pendingChoice)
        #expect(choice.package == "uv")
        #expect(choice.candidates == ["uv", "uvx"])
        store.answer(1)
        await running.value

        #expect(store.pendingChoice == nil)
        #expect(store.installed.map(\.name) == ["uv"])
        #expect(store.log.map(\.message).contains("Linked uvx for uv"))
    }

    @Test func decliningTheBinaryChoiceInstallsNothing() async throws {
        let store = makeStore(FakeKetchCore(installed: []))

        let running = Task { await store.install("uv") }
        try await waitUntil { store.pendingChoice != nil }
        store.answer(nil)
        await running.value

        #expect(store.installed.isEmpty)
        #expect(store.errorMessage == nil)
    }

    @Test func cancelWhileTheSheetIsOpenReleasesTheCore() async throws {
        let store = makeStore(FakeKetchCore(installed: []))

        let running = Task { await store.install("uv") }
        try await waitUntil { store.pendingChoice != nil }
        store.cancel()
        await running.value

        #expect(store.pendingChoice == nil)
        #expect(store.installed.isEmpty)
    }

    @Test func outdatedCountFeedsTheMenuBarAndFallsWithEachUpgrade() async {
        let store = makeStore()

        await store.refresh()
        #expect(store.pendingUpgradeCount == 2)
        #expect(store.outdatedVersion(of: "bat") == "0.25.0")

        await store.upgrade(["bat"])
        #expect(store.pendingUpgradeCount == 1)
        #expect(store.installed.first { $0.name == "bat" }?.version == "0.25.0")

        await store.upgrade()
        #expect(store.pendingUpgradeCount == 0)
    }

    @Test func updatesLeaveOutWhatALockfilePins() async throws {
        let store = makeStore()

        await store.refresh()

        #expect(store.updates.map(\.name).sorted() == ["bat", "ripgrep"])
        let pin = try #require(store.pinned.first)
        #expect(store.pinned.count == 1)
        #expect(pin.name == "jq")
        #expect(pin.heldBy == "~/work/site/ketch.lock")
        #expect(store.pendingUpgradeCount == 2)
        #expect(store.outdatedVersion(of: "jq") == nil)
    }

    @Test func upgradeAllLeavesPinnedPackagesAlone() async {
        let store = makeStore()
        await store.refresh()

        await store.upgrade()

        #expect(store.pendingUpgradeCount == 0)
        #expect(store.pinned.map(\.name) == ["jq"])
        #expect(store.installed.first { $0.name == "jq" }?.version == "1.7.1")
    }

    @Test func retryWhileTheLockIsStillHeldNamesTheNewHolder() async throws {
        let core = FakeKetchCore()
        let store = makeStore(core)
        await store.refresh()
        core.holdLock(pid: 4242)

        await store.upgrade(["bat"])
        #expect(try #require(store.busy).pid == 4242)

        core.holdLock(pid: 5151)
        await store.retryBusy()
        #expect(try #require(store.busy).pid == 5151)
        #expect(store.outdatedVersion(of: "bat") == "0.25.0")

        core.holdLock(pid: nil)
        await store.retryBusy()
        #expect(store.busy == nil)
        #expect(store.outdatedVersion(of: "bat") == nil)
        #expect(store.pendingUpgradeCount == 1)
    }

    @Test func retryWithNothingRefusedDoesNothing() async {
        let core = FakeKetchCore()
        let store = makeStore(core)

        await store.retryBusy()

        #expect(store.busy == nil)
        #expect(core.calls.isEmpty)
    }

    @Test func problemCountLeavesOutPassedChecks() async {
        let store = makeStore()

        await store.runDoctor()

        #expect(store.findings.count == 3)
        #expect(store.problemCount == 1)
    }

    @Test func uninstallRemovesThePackage() async {
        let store = makeStore()
        await store.refresh()

        await store.uninstall(["fd"])

        #expect(!store.installed.contains { $0.name == "fd" })
    }

    @Test func aSecondOperationIsRefusedWhileOneRuns() async throws {
        let core = FakeKetchCore(stepDelay: .milliseconds(30), installed: [])
        let store = makeStore(core)

        let running = Task { await store.install("jq") }
        try await waitUntil { store.isRunning }
        await store.install("fd")
        #expect(store.errorMessage != nil)
        store.cancel()
        await running.value

        #expect(!core.calls.contains("install fd"))
    }

    @Test func searchFiltersTheRegistry() async {
        let store = makeStore()

        await store.search("json")

        #expect(store.searchResults.map(\.name) == ["jq"])
    }

    @Test func doctorLoadsFindings() async {
        let store = makeStore()

        await store.runDoctor()

        #expect(store.findings.contains { $0.severity == .warning && $0.fix != nil })
    }

    @Test func changelogHeadingsLoseTheirMarkersAndLinksSurvive() {
        let text = KetchStore.render(markdown: "## 1.2.0\n\n- see [docs](https://example.com)")

        let plain = String(text.characters)
        #expect(plain == "1.2.0\n\n- see docs")
        #expect(text.runs.contains { $0.link == URL(string: "https://example.com") })
    }
}

@MainActor
@Suite struct AppSettingsTests {
    @Test func defaultsAreHourlyAndStableOnly() {
        let settings = AppSettings(defaults: scratchDefaults())

        #expect(settings.updateCheckIntervalMinutes == 60)
        #expect(settings.includePrereleases == false)
        #expect(settings.updateCheckInterval == .seconds(3600))
    }

    @Test func changesPersistAcrossInstances() {
        let defaults = scratchDefaults()
        let first = AppSettings(defaults: defaults)
        first.updateCheckIntervalMinutes = 15
        first.includePrereleases = true

        let second = AppSettings(defaults: defaults)

        #expect(second.updateCheckIntervalMinutes == 15)
        #expect(second.includePrereleases == true)
    }
}

@Suite struct KetchRootTests {
    private let home = URL(fileURLWithPath: "/Users/someone", isDirectory: true)

    @Test func ketchRootWinsWhenSet() {
        let root = KetchRoot.resolve(environment: ["KETCH_ROOT": "/tmp/scratch"], home: home)
        #expect(root.path == "/tmp/scratch")
    }

    @Test func emptyKetchRootCountsAsUnset() {
        let root = KetchRoot.resolve(environment: ["KETCH_ROOT": ""], home: home)
        #expect(root.path == "/Users/someone/.ketch")
    }

    @Test func defaultIsDotKetchInHome() {
        let root = KetchRoot.resolve(environment: [:], home: home)
        #expect(root.path == "/Users/someone/.ketch")
        #expect(KetchRoot.configFile(in: root).path == "/Users/someone/.ketch/config.toml")
    }
}

/// A UserDefaults suite no other test shares.
func scratchDefaults() -> UserDefaults {
    let name = "ketch-tests-\(UUID().uuidString)"
    let defaults = UserDefaults(suiteName: name) ?? .standard
    defaults.removePersistentDomain(forName: name)
    return defaults
}

/// Polls `condition` on the main actor until it holds, for flows that pause
/// mid-operation (a sheet, a slow step).
@MainActor
func waitUntil(timeout: Duration = .seconds(5), _ condition: () -> Bool) async throws {
    let clock = ContinuousClock()
    let deadline = clock.now + timeout
    while !condition() {
        guard clock.now < deadline else {
            Issue.record("condition not met within \(timeout)")
            return
        }
        try await Task.sleep(for: .milliseconds(10))
    }
}
