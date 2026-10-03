// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The store's states: what a page shows while an operation runs, fails,
// asks, or finds the lock held.

using Ketch.AppCore;

namespace Ketch.AppCore.Tests;

[TestClass]
public sealed class KetchStoreTests
{
    [TestMethod]
    public async Task Refresh_splits_updates_from_held_packages()
    {
        var store = new KetchStore(new FakeKetchCore());

        await store.RefreshAsync();

        Assert.AreEqual(2, store.PendingUpgradeCount);
        CollectionAssert.AreEqual(new[] { "jq" }, store.Held.Select(u => u.Name).ToList());
        Assert.AreEqual("14.1.1", store.OutdatedVersion("ripgrep"));
        Assert.IsNull(store.OutdatedVersion("jq"));
    }

    [TestMethod]
    public async Task Install_logs_its_stages_and_refreshes_the_list()
    {
        var store = new KetchStore(new FakeKetchCore());

        await store.InstallAsync("zoxide");

        Assert.IsTrue(store.Installed.Any(p => p.Name == "zoxide"));
        Assert.IsNull(store.Activity);
        Assert.IsTrue(store.Log.Any(e => e.Message == "zoxide: download"));
        Assert.AreEqual(LogLevel.Success, store.Log.Last().Level);
    }

    [TestMethod]
    public async Task An_unknown_package_shows_the_cores_wording_as_an_alert()
    {
        var store = new KetchStore(new FakeKetchCore());

        await store.InstallAsync("nope");

        Assert.AreEqual("No package named nope.", store.ErrorMessage);
        Assert.IsNull(store.Busy);
    }

    [TestMethod]
    public async Task A_held_lock_shows_the_busy_banner_and_retry_runs_the_install_again()
    {
        var core = new FakeKetchCore();
        core.HoldLock(4242);
        var store = new KetchStore(core);

        await store.InstallAsync("zoxide");

        Assert.AreEqual(4242u, store.Busy?.Pid);
        Assert.IsNull(store.ErrorMessage);

        core.HoldLock(null, held: false);
        await store.RetryBusyAsync();

        Assert.IsNull(store.Busy);
        Assert.IsTrue(store.Installed.Any(p => p.Name == "zoxide"));
    }

    [TestMethod]
    public async Task A_lock_that_stays_held_brings_the_banner_back_with_the_new_holder()
    {
        var core = new FakeKetchCore();
        core.HoldLock(1);
        var store = new KetchStore(core);
        await store.InstallAsync("zoxide");

        core.HoldLock(2);
        await store.RetryBusyAsync();

        Assert.AreEqual(2u, store.Busy?.Pid);
    }

    [TestMethod]
    public async Task A_cancelled_operation_is_logged_and_not_alerted()
    {
        var core = new FakeKetchCore();
        core.Script(Scenarios.Named("install-cancelled"));
        var store = new KetchStore(core);

        await store.InstallAsync("ripgrep");

        Assert.IsNull(store.ErrorMessage);
        Assert.IsTrue(store.Log.Any(e => e.Message == "Cancelled" && e.Level == LogLevel.Warning));
    }

    [TestMethod]
    public async Task The_binary_choice_waits_for_an_answer_from_the_dialog()
    {
        var store = new KetchStore(new FakeKetchCore());

        var install = store.InstallAsync("uv");
        await Wait.Until(() => store.PendingChoice is not null);
        Assert.AreEqual("uv", store.PendingChoice!.Package);
        store.AnswerChoice(1);
        await install;

        Assert.IsNull(store.PendingChoice);
        Assert.IsTrue(store.Installed.Any(p => p.Name == "uv"));
    }

    [TestMethod]
    public async Task Cancelling_while_a_question_is_open_declines_it_and_stops_the_operation()
    {
        var store = new KetchStore(new FakeKetchCore());

        var install = store.InstallAsync("uv");
        await Wait.Until(() => store.PendingChoice is not null);
        store.Cancel();
        await install;

        Assert.IsNull(store.PendingChoice);
        Assert.IsNull(store.ErrorMessage);
        Assert.IsFalse(store.Installed.Any(p => p.Name == "uv"));
    }

    [TestMethod]
    public async Task The_processes_question_waits_for_an_answer_from_the_dialog()
    {
        var core = new FakeKetchCore();
        core.Script(Scenarios.Named("upgrade-stops-processes"));
        var store = new KetchStore(core);

        var upgrade = store.UpgradeAsync(["ripgrep"]);
        await Wait.Until(() => store.PendingStop is not null);
        Assert.AreEqual(5150u, store.PendingStop!.Holders.Single().Pid);
        store.AnswerStop(true);
        await upgrade;

        Assert.IsNull(store.ErrorMessage);
        Assert.AreEqual("14.1.1", store.Installed.Single(p => p.Name == "ripgrep").Version);
    }

    [TestMethod]
    public async Task A_second_operation_while_one_runs_is_ignored()
    {
        var store = new KetchStore(new FakeKetchCore());

        var first = store.InstallAsync("uv");
        await Wait.Until(() => store.PendingChoice is not null);
        await store.UninstallAsync(["bat"]);
        store.AnswerChoice(0);
        await first;

        Assert.IsTrue(store.Installed.Any(p => p.Name == "bat"));
    }

    [TestMethod]
    public async Task Events_reach_the_store_through_the_ui_dispatcher()
    {
        var posted = 0;
        var store = new KetchStore(new FakeKetchCore(), action =>
        {
            Interlocked.Increment(ref posted);
            action();
        });

        await store.InstallAsync("zoxide");

        Assert.IsTrue(posted > 0);
    }

    [TestMethod]
    public async Task Doctor_counts_the_findings_that_are_not_ok()
    {
        var store = new KetchStore(new FakeKetchCore());

        await store.RunDoctorAsync();

        Assert.AreEqual(1, store.ProblemCount);
    }

    [TestMethod]
    public async Task Search_with_an_empty_query_lists_the_registry()
    {
        var store = new KetchStore(new FakeKetchCore());

        await store.SearchAsync("");

        Assert.AreEqual(FakeKetchCore.SampleRegistry.Count, store.SearchResults.Count);
    }

    [TestMethod]
    public void A_downloads_share_of_the_bar_grows_with_the_bytes_received()
    {
        var half = new PackageProgress(Stage.Download) { Done = 50, Total = 100 };
        var start = new PackageProgress(Stage.Download) { Done = 0, Total = 100 };

        Assert.IsTrue(half.Fraction > start.Fraction);
        Assert.IsTrue(half.Fraction < new PackageProgress(Stage.Verify).Fraction);
    }
}
