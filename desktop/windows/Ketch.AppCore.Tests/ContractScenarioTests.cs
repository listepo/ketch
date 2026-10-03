// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The scenario files decode and map onto the app's types the way the macOS
// fake's tests expect, so a field the generator renames fails here.

using Ketch.AppCore;

namespace Ketch.AppCore.Tests;

[TestClass]
public sealed class ContractScenarioTests
{
    [TestMethod]
    public void Every_scenario_file_decodes()
    {
        var files = Directory.GetFiles(ContractScenario.DefaultDirectory, "*.json");

        Assert.IsGreaterThanOrEqualTo(18, files.Length);
        Assert.HasCount(files.Length, Scenarios.Every);
        foreach (var scenario in Scenarios.Every)
        {
            Assert.IsFalse(string.IsNullOrEmpty(scenario.Description), scenario.Name);
        }
    }

    [TestMethod]
    public void The_installed_scenario_maps_a_github_source_to_its_repo()
    {
        var packages = Scenarios.Named("installed").Value<List<ContractScenario.Package>>().Select(p => p.AsInstalled()).ToList();

        Assert.IsTrue(packages.Count > 0);
        Assert.IsTrue(packages.All(p => p.Source == "github" && p.Repo is not null));
    }

    [TestMethod]
    public void The_outdated_scenario_keeps_a_held_package_apart()
    {
        var upgrades = Scenarios.Named("outdated").Value<List<ContractScenario.UpgradeEntry>>().Select(u => u.AsUpgrade()).ToList();

        Assert.IsTrue(upgrades.Any(u => u.HeldBy is not null));
        Assert.IsTrue(upgrades.Any(u => u.HeldBy is null));
    }

    [TestMethod]
    public void The_doctor_scenario_maps_every_outcome_to_a_severity()
    {
        var findings = Scenarios.Named("doctor").Value<List<ContractScenario.CheckEntry>>().Select(c => c.AsFinding()).ToList();

        Assert.IsTrue(findings.Any(f => f.Severity == Severity.Ok));
        Assert.IsTrue(findings.Any(f => f.Severity != Severity.Ok && f.Fix is not null));
    }

    [TestMethod]
    public void The_changelog_scenario_reads_as_markdown_headed_by_version()
    {
        var markdown = ContractScenario.Markdown(Scenarios.Named("changelog-range").Value<List<ContractScenario.ChangelogEntry>>());

        StringAssert.StartsWith(markdown, "## ");
    }

    [TestMethod]
    public void A_busy_error_carries_the_pid_and_an_unknown_holder_has_none()
    {
        var known = Assert.ThrowsExactly<KetchException>(() => Scenarios.Named("install-busy").Value<List<ContractScenario.Installed>>());
        var unknown = Assert.ThrowsExactly<KetchException>(() => Scenarios.Named("install-busy-unknown-holder").Value<List<ContractScenario.Installed>>());

        Assert.AreEqual(KetchErrorKind.Busy, known.Kind);
        Assert.AreEqual(4242u, known.Pid);
        Assert.IsNull(unknown.Pid);
    }

    [TestMethod]
    [DataRow("install-not-found", KetchErrorKind.NotFound)]
    [DataRow("install-network-failure", KetchErrorKind.Network)]
    [DataRow("install-verification-failure", KetchErrorKind.Verification)]
    [DataRow("install-other-error", KetchErrorKind.Other)]
    [DataRow("install-cancelled", KetchErrorKind.Cancelled)]
    public void An_error_scenario_throws_the_matching_kind(string name, KetchErrorKind kind)
    {
        var error = Assert.ThrowsExactly<KetchException>(() => Scenarios.Named(name).Value<List<ContractScenario.Installed>>());

        Assert.AreEqual(kind, error.Kind);
    }

    [TestMethod]
    public void The_event_mapper_keys_progress_by_the_package_a_task_downloads_for()
    {
        var mapper = new ContractEventMapper();
        var events = Scenarios.Named("install-ok").Script
            .Where(s => s.Event is not null)
            .SelectMany(s => mapper.Map(s.Event!))
            .ToList();

        Assert.IsTrue(events.OfType<CoreEvent.Step>().Any(s => s.Stage == Stage.Resolve));
        Assert.IsTrue(events.OfType<CoreEvent.Progress>().All(p => p.Package == "ripgrep"));
        Assert.IsTrue(events.OfType<CoreEvent.Progress>().Any());
    }
}
