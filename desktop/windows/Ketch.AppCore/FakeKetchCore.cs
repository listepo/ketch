// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// A stand-in for the real core: canned packages, simulated pipeline stages and
// progress, and switches for the states the UI must handle (a held lock, a
// package with several binaries). Previews, tests and, until the binding is
// wired, the app itself run on it. It never touches the disk, and it ports
// `FakeKetchCore.swift` so the macOS and Windows fakes behave alike.

namespace Ketch.AppCore;

public sealed class FakeKetchCore : IKetchCore
{
    private readonly object gate = new();
    private readonly TimeSpan stepDelay;
    private readonly List<InstalledPackage> installed;
    private List<RegistryPackage> registry;
    private readonly Dictionary<string, IReadOnlyList<string>> binaries;
    private readonly Dictionary<string, string> pins;
    private readonly Dictionary<string, ContractScenario> scripted = [];
    private readonly List<string> calls = [];
    private uint? lockHolder;
    private bool lockHeld;
    private IReadOnlyList<Upgrade>? upgrades;
    private IReadOnlyList<Finding>? findings;
    private string? changelogText;

    public FakeKetchCore(
        string root = @"C:\ketch-fake",
        TimeSpan stepDelay = default,
        IEnumerable<InstalledPackage>? installed = null,
        IEnumerable<RegistryPackage>? registry = null,
        IReadOnlyDictionary<string, IReadOnlyList<string>>? binaries = null,
        IReadOnlyDictionary<string, string>? pins = null)
    {
        Root = root;
        this.stepDelay = stepDelay;
        this.installed = [.. installed ?? SampleInstalled];
        this.registry = [.. registry ?? SampleRegistry];
        this.binaries = new Dictionary<string, IReadOnlyList<string>>(
            binaries ?? new Dictionary<string, IReadOnlyList<string>> { ["uv"] = ["uv", "uvx"] });
        this.pins = new Dictionary<string, string>(pins ?? SamplePins);
    }

    public string Root { get; }

    /// <summary>Every operation called, in order, for tests to assert on.</summary>
    public IReadOnlyList<string> Calls
    {
        get
        {
            lock (gate)
            {
                return [.. calls];
            }
        }
    }

    /// <summary>Simulates another ketch process holding the lock; <paramref name="holder"/> null with <paramref name="held"/> false releases it.</summary>
    public void HoldLock(uint? holder, bool held = true)
    {
        lock (gate)
        {
            lockHolder = holder;
            lockHeld = held;
        }
    }

    /// <summary>
    /// Makes the fake answer as the core did in <paramref name="scenario"/>: a
    /// read returns the scenario's records, and install, upgrade and uninstall
    /// replay its events, questions and outcome.
    /// </summary>
    public void Script(ContractScenario scenario)
    {
        lock (gate)
        {
            switch (scenario.Call.Operation)
            {
                case "installed":
                    installed.Clear();
                    installed.AddRange(scenario.Value<List<ContractScenario.Package>>().Select(p => p.AsInstalled()));
                    break;
                case "search":
                    registry = [.. scenario.Value<ContractScenario.SearchResults>().Known.Select(e => e.AsRegistryPackage())];
                    break;
                case "outdated":
                    upgrades = [.. scenario.Value<List<ContractScenario.UpgradeEntry>>().Select(u => u.AsUpgrade())];
                    break;
                case "doctor":
                    findings = [.. scenario.Value<List<ContractScenario.CheckEntry>>().Select(c => c.AsFinding())];
                    break;
                case "changelog_range":
                    changelogText = ContractScenario.Markdown(scenario.Value<List<ContractScenario.ChangelogEntry>>());
                    break;
                case "install" or "upgrade" or "uninstall":
                    scripted[scenario.Call.Operation] = scenario;
                    break;
                default:
                    throw KetchException.Other($"no fake behaviour for the scenario operation {scenario.Call.Operation}");
            }
        }
    }

    /// <summary>Goes back to the simulated pipeline for install, upgrade and uninstall.</summary>
    public void ClearScripted()
    {
        lock (gate)
        {
            scripted.Clear();
        }
    }

    public IReadOnlyList<InstalledPackage> Installed()
    {
        lock (gate)
        {
            calls.Add("installed");
            return [.. installed.OrderBy(p => p.Name, StringComparer.Ordinal)];
        }
    }

    public IReadOnlyList<RegistryPackage> Search(string query)
    {
        Pause();
        var needle = query.Trim().ToLowerInvariant();
        lock (gate)
        {
            calls.Add($"search {needle}");
            return needle.Length == 0
                ? [.. registry]
                : [.. registry.Where(p =>
                    p.Name.Contains(needle, StringComparison.Ordinal)
                    || (p.Description?.Contains(needle, StringComparison.OrdinalIgnoreCase) ?? false))];
        }
    }

    public IReadOnlyList<Upgrade> Outdated()
    {
        lock (gate)
        {
            calls.Add("outdated");
            if (upgrades is { } fixedUpgrades)
            {
                return fixedUpgrades;
            }

            return [.. installed.Select(package =>
            {
                var latest = registry.FirstOrDefault(r => r.Name == package.Name)?.Latest;
                return latest is null || latest == package.Version
                    ? null
                    : new Upgrade(package.Name, package.Version, latest, pins.GetValueOrDefault(package.Name));
            }).OfType<Upgrade>()];
        }
    }

    public void Install(string spec, InstallOptions options, IReporter reporter, IDecider decider, CancelToken cancel)
    {
        if (Scripted("install") is { } scenario)
        {
            Record($"install {spec}");
            Replay(scenario, reporter, decider, cancel);
            return;
        }

        var at = spec.IndexOf('@');
        var name = at < 0 ? spec : spec[..at];
        var pinned = at >= 0 && at < spec.Length - 1 ? spec[(at + 1)..] : null;
        RegistryPackage entry;
        lock (gate)
        {
            calls.Add($"install {spec}");
            ThrowIfLocked();
            entry = registry.FirstOrDefault(r => r.Name == name) ?? throw KetchException.NotFound(name);
        }

        var version = pinned ?? entry.Latest ?? "0.0.0";
        RunPipeline(name, reporter, decider, cancel);
        lock (gate)
        {
            installed.RemoveAll(p => p.Name == name);
            installed.Add(new InstalledPackage(
                name, version, "github", entry.Repo, entry.Description, Path.Combine(Root, "store", name)));
        }

        reporter.Event(new CoreEvent.Status($"Installed {name} {version}"));
    }

    public void Upgrade(IReadOnlyList<string> names, IReporter reporter, IDecider decider, CancelToken cancel)
    {
        var label = $"upgrade {string.Join(' ', names)}";
        if (Scripted("upgrade") is { } scenario)
        {
            Record(label);
            Replay(scenario, reporter, decider, cancel);
            return;
        }

        CheckLock(label);
        var targets = Outdated().Where(u => u.HeldBy is null && (names.Count == 0 || names.Contains(u.Name))).ToList();
        if (targets.Count == 0)
        {
            reporter.Event(new CoreEvent.Status("Everything is up to date"));
        }

        foreach (var target in targets)
        {
            RunPipeline(target.Name, reporter, decider, cancel);
            lock (gate)
            {
                var index = installed.FindIndex(p => p.Name == target.Name);
                if (index >= 0)
                {
                    installed[index] = installed[index] with { Version = target.To };
                }
            }

            reporter.Event(new CoreEvent.Status($"Upgraded {target.Name} {target.From} → {target.To}"));
        }
    }

    public void Uninstall(IReadOnlyList<string> names, IReporter reporter, CancelToken cancel)
    {
        var label = $"uninstall {string.Join(' ', names)}";
        if (Scripted("uninstall") is { } scenario)
        {
            Record(label);
            Replay(scenario, reporter, new NoDecider(), cancel);
            return;
        }

        CheckLock(label);
        foreach (var name in names)
        {
            if (cancel.IsCancelled)
            {
                throw KetchException.Cancelled();
            }

            Pause();
            lock (gate)
            {
                if (installed.RemoveAll(p => p.Name == name) == 0)
                {
                    throw KetchException.NotFound(name);
                }
            }

            reporter.Event(new CoreEvent.Status($"Uninstalled {name}"));
        }
    }

    public string Changelog(string name, string? from, string? to)
    {
        Pause();
        lock (gate)
        {
            calls.Add($"changelog {name}");
            if (changelogText is { } text)
            {
                return text;
            }
        }

        return $"""
            ## {to ?? "latest"}

            ### Features

            - **Faster search** across large trees.
            - New `--json` output, see [the docs](https://github.com/pyrlyn/ketch).

            ### Fixes

            - Handles paths with spaces.

            ## {from ?? "previous"}

            - Initial release notes for {name}.
            """;
    }

    public IReadOnlyList<Finding> Doctor()
    {
        Pause();
        lock (gate)
        {
            calls.Add("doctor");
            return findings ??
            [
                new Finding("root", Severity.Ok, $"ketch root is {Root}", null),
                new Finding("path", Severity.Warning, "The bin dir is not on the user PATH", "Add to PATH"),
                new Finding("links", Severity.Ok, "All links resolve", null),
            ];
        }
    }

    private void Record(string call)
    {
        lock (gate)
        {
            calls.Add(call);
        }
    }

    private ContractScenario? Scripted(string operation)
    {
        lock (gate)
        {
            return scripted.GetValueOrDefault(operation);
        }
    }

    private void ThrowIfLocked()
    {
        if (lockHeld)
        {
            throw KetchException.Busy(lockHolder);
        }
    }

    private void CheckLock(string call)
    {
        lock (gate)
        {
            calls.Add(call);
            ThrowIfLocked();
        }
    }

    private void RunPipeline(string name, IReporter reporter, IDecider decider, CancelToken cancel)
    {
        const ulong total = 4_000_000;
        foreach (var stage in Enum.GetValues<Stage>())
        {
            if (cancel.IsCancelled)
            {
                throw KetchException.Cancelled();
            }

            reporter.Event(new CoreEvent.Step(name, stage));
            switch (stage)
            {
                case Stage.Download:
                    for (ulong chunk = 0; chunk <= 4; chunk++)
                    {
                        if (cancel.IsCancelled)
                        {
                            throw KetchException.Cancelled();
                        }

                        reporter.Event(new CoreEvent.Progress(name, total / 4 * chunk, total));
                        Pause();
                    }

                    break;
                case Stage.Link:
                    IReadOnlyList<string> candidates;
                    lock (gate)
                    {
                        candidates = binaries.GetValueOrDefault(name) ?? [];
                    }

                    if (candidates.Count > 1)
                    {
                        if (decider.ChooseBinary(name, candidates) is not { } choice || choice < 0 || choice >= candidates.Count)
                        {
                            throw KetchException.Cancelled();
                        }

                        reporter.Event(new CoreEvent.Status($"Linked {candidates[choice]} for {name}"));
                    }

                    Pause();
                    break;
                default:
                    Pause();
                    break;
            }
        }
    }

    /// <summary>
    /// Plays a contract scenario: its events to the reporter, its questions to
    /// the decider (which answers, not the file), then its outcome. A cancel is
    /// honoured between steps, as the real pipeline does.
    /// </summary>
    private void Replay(ContractScenario scenario, IReporter reporter, IDecider decider, CancelToken cancel)
    {
        var mapper = new ContractEventMapper();
        foreach (var step in scenario.Script)
        {
            if (cancel.IsCancelled)
            {
                throw KetchException.Cancelled();
            }

            if (step.Event is { } e)
            {
                foreach (var mapped in mapper.Map(e))
                {
                    reporter.Event(mapped);
                }
            }
            else if (step.Question is { } question)
            {
                Ask(question, decider);
            }

            Pause();
        }

        lock (gate)
        {
            if (scenario.Call.Operation == "uninstall")
            {
                var removed = scenario.Value<List<ContractScenario.Package>>().Select(p => p.Name).ToHashSet();
                installed.RemoveAll(p => removed.Contains(p.Name));
                return;
            }

            foreach (var package in scenario.Value<List<ContractScenario.Installed>>().Select(i => i.Package.AsInstalled()))
            {
                installed.RemoveAll(p => p.Name == package.Name);
                installed.Add(package);
            }
        }
    }

    private void Ask(ContractScenario.Question question, IDecider decider)
    {
        switch (question.Type)
        {
            case "choose_binary" when question.Package is { } package && question.Candidates is { } candidates:
                var pick = decider.ChooseBinary(package, candidates);
                Record($"decision {package} {(pick is { } p ? p.ToString() : "none")}");
                if (pick is not { } index || index < 0 || index >= candidates.Count)
                {
                    throw KetchException.Cancelled();
                }

                break;
            case "stop_processes" when question.Holders is { } holders:
                var holding = holders.Select(h => new ProcessHolder(h.Pid, h.Path)).ToList();
                var stop = decider.StopProcesses(holding);
                Record($"decision stop {(stop ? "yes" : "no")}");
                if (!stop)
                {
                    throw KetchException.Cancelled();
                }

                break;
        }
    }

    private void Pause()
    {
        if (stepDelay > TimeSpan.Zero)
        {
            Thread.Sleep(stepDelay);
        }
    }

    /// <summary>For an operation that never asks.</summary>
    private sealed class NoDecider : IDecider
    {
        public int? ChooseBinary(string package, IReadOnlyList<string> candidates) => null;

        public bool StopProcesses(IReadOnlyList<ProcessHolder> holders) => false;
    }

    public static IReadOnlyList<InstalledPackage> SampleInstalled { get; } =
    [
        new("ripgrep", "14.1.0", "github", "BurntSushi/ripgrep", "Recursively search directories for a regex pattern", @"C:\ketch-fake\store\ripgrep"),
        new("fd", "10.2.0", "github", "sharkdp/fd", "A simple, fast and user-friendly alternative to find", @"C:\ketch-fake\store\fd"),
        new("bat", "0.24.0", "github", "sharkdp/bat", "A cat clone with wings", @"C:\ketch-fake\store\bat"),
        new("jq", "1.7.1", "github", "jqlang/jq", "Command-line JSON processor", @"C:\ketch-fake\store\jq"),
    ];

    /// <summary>jq is held back by a project's lockfile, so the Updates screen has a held package to show.</summary>
    public static IReadOnlyDictionary<string, string> SamplePins { get; } =
        new Dictionary<string, string> { ["jq"] = @"C:\work\site\ketch.lock" };

    public static IReadOnlyList<RegistryPackage> SampleRegistry { get; } =
    [
        new("ripgrep", "BurntSushi/ripgrep", "Recursively search directories for a regex pattern", "14.1.1"),
        new("fd", "sharkdp/fd", "A simple, fast and user-friendly alternative to find", "10.2.0"),
        new("bat", "sharkdp/bat", "A cat clone with wings", "0.25.0"),
        new("jq", "jqlang/jq", "Command-line JSON processor", "1.8.1"),
        new("uv", "astral-sh/uv", "An extremely fast Python package manager", "0.9.0"),
        new("zoxide", "ajeetdsouza/zoxide", "A smarter cd command", "0.9.8"),
    ];
}
