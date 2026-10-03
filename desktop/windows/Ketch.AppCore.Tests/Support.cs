// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// Shared helpers: the scenarios as shipped beside the test assembly, and a
// reporter and decider that record and answer.

using Ketch.AppCore;

namespace Ketch.AppCore.Tests;

internal static class Scenarios
{
    private static readonly Lazy<IReadOnlyList<ContractScenario>> All =
        new(() => ContractScenario.Load(ContractScenario.DefaultDirectory));

    public static ContractScenario Named(string name) =>
        All.Value.Single(s => s.Name == name);

    public static IReadOnlyList<ContractScenario> Every => All.Value;
}

internal sealed class RecordingReporter : IReporter
{
    private readonly List<CoreEvent> events = [];

    public IReadOnlyList<CoreEvent> Events
    {
        get
        {
            lock (events)
            {
                return [.. events];
            }
        }
    }

    public void Event(CoreEvent coreEvent)
    {
        lock (events)
        {
            events.Add(coreEvent);
        }
    }
}

/// <summary>Answers every question the same way, and remembers what it was asked.</summary>
internal sealed class ScriptedDecider(int? binary = 0, bool stop = true) : IDecider
{
    public List<string> Asked { get; } = [];

    public int? ChooseBinary(string package, IReadOnlyList<string> candidates)
    {
        Asked.Add($"binary {package} {string.Join(',', candidates)}");
        return binary;
    }

    public bool StopProcesses(IReadOnlyList<ProcessHolder> holders)
    {
        Asked.Add($"stop {string.Join(',', holders.Select(h => h.Pid))}");
        return stop;
    }
}

internal static class Wait
{
    /// <summary>Polls until <paramref name="condition"/> holds; the store's worker threads answer on their own time.</summary>
    public static async Task Until(Func<bool> condition)
    {
        var deadline = DateTime.UtcNow.AddSeconds(10);
        while (!condition())
        {
            Assert.IsTrue(DateTime.UtcNow < deadline, "timed out waiting for the condition");
            await Task.Delay(10);
        }
    }
}
