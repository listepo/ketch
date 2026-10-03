// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The fake core's behaviour, on its own samples and replaying scenarios.

using Ketch.AppCore;

namespace Ketch.AppCore.Tests;

[TestClass]
public sealed class FakeKetchCoreTests
{
    private static readonly CancelToken Never = new();

    [TestMethod]
    public void Outdated_leaves_current_packages_out_and_marks_pinned_ones_held()
    {
        var upgrades = new FakeKetchCore().Outdated();

        Assert.IsFalse(upgrades.Any(u => u.Name == "fd"));
        Assert.IsNotNull(upgrades.Single(u => u.Name == "jq").HeldBy);
        Assert.IsNull(upgrades.Single(u => u.Name == "ripgrep").HeldBy);
    }

    [TestMethod]
    public void Install_of_an_unknown_name_is_not_found()
    {
        var error = Assert.ThrowsExactly<KetchException>(
            () => new FakeKetchCore().Install("nope", new InstallOptions(), new RecordingReporter(), new ScriptedDecider(), Never));

        Assert.AreEqual(KetchErrorKind.NotFound, error.Kind);
    }

    [TestMethod]
    public void Install_asks_which_binary_to_link_when_a_package_ships_several()
    {
        var core = new FakeKetchCore();
        var decider = new ScriptedDecider(binary: 1);

        core.Install("uv", new InstallOptions(), new RecordingReporter(), decider, Never);

        CollectionAssert.AreEqual(new[] { "binary uv uv,uvx" }, decider.Asked);
        Assert.IsTrue(core.Installed().Any(p => p.Name == "uv"));
    }

    [TestMethod]
    public void Declining_the_binary_choice_cancels_the_install()
    {
        var core = new FakeKetchCore();

        var error = Assert.ThrowsExactly<KetchException>(
            () => core.Install("uv", new InstallOptions(), new RecordingReporter(), new ScriptedDecider(binary: null), Never));

        Assert.AreEqual(KetchErrorKind.Cancelled, error.Kind);
        Assert.IsFalse(core.Installed().Any(p => p.Name == "uv"));
    }

    [TestMethod]
    public void A_held_lock_refuses_a_mutation_with_the_holders_pid()
    {
        var core = new FakeKetchCore();
        core.HoldLock(77);

        var error = Assert.ThrowsExactly<KetchException>(() => core.Uninstall(["bat"], new RecordingReporter(), Never));

        Assert.AreEqual(77u, error.Pid);
    }

    [TestMethod]
    public void Upgrade_skips_held_packages_and_moves_the_rest_to_latest()
    {
        var core = new FakeKetchCore();

        core.Upgrade([], new RecordingReporter(), new ScriptedDecider(), Never);

        var installed = core.Installed().ToDictionary(p => p.Name);
        Assert.AreEqual("14.1.1", installed["ripgrep"].Version);
        Assert.AreEqual("0.25.0", installed["bat"].Version);
        Assert.AreEqual("1.7.1", installed["jq"].Version);
    }

    [TestMethod]
    public void A_cancel_before_the_first_step_stops_with_cancelled()
    {
        var cancel = new CancelToken();
        cancel.Cancel();

        var error = Assert.ThrowsExactly<KetchException>(
            () => new FakeKetchCore().Install("ripgrep", new InstallOptions(), new RecordingReporter(), new ScriptedDecider(), cancel));

        Assert.AreEqual(KetchErrorKind.Cancelled, error.Kind);
    }

    [TestMethod]
    public void A_scripted_install_replays_its_events_and_places_the_package()
    {
        var core = new FakeKetchCore();
        core.Script(Scenarios.Named("install-ok"));
        var reporter = new RecordingReporter();

        core.Install("ripgrep", new InstallOptions(), reporter, new ScriptedDecider(), Never);

        Assert.IsTrue(reporter.Events.OfType<CoreEvent.Step>().Any());
        Assert.AreEqual("14.1.1", core.Installed().Single(p => p.Name == "ripgrep").Version);
    }

    [TestMethod]
    public void A_scripted_failure_throws_after_its_events_and_places_nothing()
    {
        var core = new FakeKetchCore(installed: []);
        core.Script(Scenarios.Named("install-network-failure"));
        var reporter = new RecordingReporter();

        var error = Assert.ThrowsExactly<KetchException>(
            () => core.Install("ripgrep", new InstallOptions(), reporter, new ScriptedDecider(), Never));

        Assert.AreEqual(KetchErrorKind.Network, error.Kind);
        Assert.IsTrue(reporter.Events.Count > 0);
        Assert.AreEqual(0, core.Installed().Count);
    }

    [TestMethod]
    public void A_scripted_binary_choice_goes_to_the_decider_not_to_the_file()
    {
        var core = new FakeKetchCore();
        core.Script(Scenarios.Named("install-binary-choice"));
        var decider = new ScriptedDecider(binary: 1);

        core.Install("uv", new InstallOptions(), new RecordingReporter(), decider, Never);

        Assert.HasCount(1, decider.Asked);
        StringAssert.StartsWith(decider.Asked[0], "binary uv");
        CollectionAssert.Contains(core.Calls.ToList(), "decision uv 1");
    }

    [TestMethod]
    public void A_scripted_upgrade_asks_before_stopping_the_processes_that_hold_the_files()
    {
        var core = new FakeKetchCore();
        core.Script(Scenarios.Named("upgrade-stops-processes"));
        var decider = new ScriptedDecider(stop: true);

        core.Upgrade(["ripgrep"], new RecordingReporter(), decider, Never);

        CollectionAssert.AreEqual(new[] { "stop 5150" }, decider.Asked);
    }

    [TestMethod]
    public void Refusing_to_stop_the_processes_cancels_the_upgrade()
    {
        var core = new FakeKetchCore();
        core.Script(Scenarios.Named("upgrade-stops-processes"));

        var error = Assert.ThrowsExactly<KetchException>(
            () => core.Upgrade(["ripgrep"], new RecordingReporter(), new ScriptedDecider(stop: false), Never));

        Assert.AreEqual(KetchErrorKind.Cancelled, error.Kind);
    }

    [TestMethod]
    public void A_scripted_uninstall_removes_the_package_it_names()
    {
        var core = new FakeKetchCore();
        core.Script(Scenarios.Named("uninstall-ok"));

        core.Uninstall(["ripgrep"], new RecordingReporter(), Never);

        Assert.IsFalse(core.Installed().Any(p => p.Name == "ripgrep"));
    }

    [TestMethod]
    public void Clearing_the_scripts_goes_back_to_the_simulated_pipeline()
    {
        var core = new FakeKetchCore();
        core.Script(Scenarios.Named("install-not-found"));
        core.ClearScripted();

        core.Install("zoxide", new InstallOptions(), new RecordingReporter(), new ScriptedDecider(), Never);

        Assert.IsTrue(core.Installed().Any(p => p.Name == "zoxide"));
    }

    [TestMethod]
    public void A_scripted_read_replaces_what_the_fake_answers()
    {
        var core = new FakeKetchCore();
        core.Script(Scenarios.Named("outdated"));
        core.Script(Scenarios.Named("doctor"));

        Assert.AreEqual(
            Scenarios.Named("outdated").Value<List<ContractScenario.UpgradeEntry>>().Count,
            core.Outdated().Count);
        Assert.IsTrue(core.Doctor().Any(f => f.Id == "path"));
    }

    [TestMethod]
    public void Search_matches_names_and_descriptions_case_insensitively()
    {
        var names = new FakeKetchCore().Search("PYTHON").Select(p => p.Name).ToList();

        CollectionAssert.AreEqual(new[] { "uv" }, names);
    }
}
