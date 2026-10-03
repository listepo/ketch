// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The fake core the app runs on until the binding is wired: the contract
// scenarios decide what the reads return, and Settings can script an install,
// upgrade or uninstall from one to reach a state (busy, a failed download, a
// question) that the simulated pipeline never produces.

using Ketch.AppCore;

namespace KetchApp;

internal static class FakeCoreSetup
{
    /// <summary>Reads that the app shows from the scenarios when it starts.</summary>
    private static readonly string[] Reads = ["installed", "search", "outdated", "doctor", "changelog_range"];

    /// <summary>A short pause per step, so progress and the busy state can be seen.</summary>
    private static readonly TimeSpan StepDelay = TimeSpan.FromMilliseconds(350);

    public static FakeKetchCore Create()
    {
        var core = new FakeKetchCore(stepDelay: StepDelay);
        foreach (var scenario in Scenarios())
        {
            if (Reads.Contains(scenario.Call.Operation))
            {
                core.Script(scenario);
            }
        }

        return core;
    }

    /// <summary>Every scenario shipped beside the app; none when the folder is missing, so the app still runs on the samples.</summary>
    public static IReadOnlyList<ContractScenario> Scenarios()
    {
        try
        {
            return ContractScenario.Load(ContractScenario.DefaultDirectory);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or InvalidDataException or System.Text.Json.JsonException)
        {
            return [];
        }
    }

    /// <summary>Whether a scenario is one Settings can script (a mutation, not a read).</summary>
    public static bool IsMutation(ContractScenario scenario) =>
        scenario.Call.Operation is "install" or "upgrade" or "uninstall";
}
