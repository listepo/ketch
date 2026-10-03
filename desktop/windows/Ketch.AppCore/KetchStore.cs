// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// What every page binds to: the installed packages, the updates, the running
// operation, the log and the questions the core is waiting on. It ports the
// macOS `KetchStore` without its settings, tray or link handling.
//
// The core blocks, so each call runs on its own thread; events and questions
// come back on that thread and are handed to `post`, which the app points at
// its DispatcherQueue so every property changes on the UI thread, in the
// order the core sent them.

using System.ComponentModel;
using System.Runtime.CompilerServices;

namespace Ketch.AppCore;

/// <summary>A running operation's progress, per package.</summary>
public sealed class Activity(string title)
{
    public string Title { get; } = title;

    public Dictionary<string, PackageProgress> Packages { get; } = [];

    public string? Status { get; set; }

    public bool IsCancelling { get; set; }

    /// <summary>Overall progress in 0..1; the stages count for the part of a package's bar that is not download.</summary>
    public double Fraction
    {
        get
        {
            if (Packages.Count == 0)
            {
                return 0;
            }

            return Packages.Values.Average(p => p.Fraction);
        }
    }
}

public sealed class PackageProgress(Stage stage)
{
    public Stage Stage { get; set; } = stage;

    public ulong Done { get; set; }

    public ulong? Total { get; set; }

    /// <summary>
    /// Each of the six stages is a sixth of the bar; download fills its sixth
    /// as bytes arrive when the total is known.
    /// </summary>
    public double Fraction
    {
        get
        {
            var stages = Enum.GetValues<Stage>().Length;
            var within = Stage == Stage.Download && Total is > 0 ? Math.Min(1.0, (double)Done / Total.Value) : 0;
            return (((int)Stage) + within) / stages;
        }
    }
}

public enum LogLevel { Info, Success, Warning, Error }

public sealed record LogEntry(long Id, DateTimeOffset Date, LogLevel Level, string Message);

/// <summary>A binary choice the core is waiting on.</summary>
public sealed class BinaryChoice(string package, IReadOnlyList<string> candidates, Action<int?> answer)
{
    public string Package { get; } = package;

    public IReadOnlyList<string> Candidates { get; } = candidates;

    internal Action<int?> Answer { get; } = answer;
}

/// <summary>A question about stopping processes that hold files the operation replaces.</summary>
public sealed class StopQuestion(IReadOnlyList<ProcessHolder> holders, Action<bool> answer)
{
    public IReadOnlyList<ProcessHolder> Holders { get; } = holders;

    internal Action<bool> Answer { get; } = answer;
}

/// <summary>Another ketch process holds the lock; Retry runs what it refused again.</summary>
public sealed class BusyState(uint? pid, Func<Task> retry)
{
    public uint? Pid { get; } = pid;

    public string Message { get; } = KetchException.Busy(pid).Message;

    internal Func<Task> Retry { get; } = retry;
}

public sealed class KetchStore : INotifyPropertyChanged
{
    private const int LogLimit = 500;

    private readonly IKetchCore core;
    private readonly Action<Action> post;
    private CancelToken? cancelToken;
    private long nextLogId;

    private IReadOnlyList<InstalledPackage> installed = [];
    private IReadOnlyList<Upgrade> outdated = [];
    private IReadOnlyList<RegistryPackage> searchResults = [];
    private IReadOnlyList<Finding> findings = [];
    private IReadOnlyList<LogEntry> log = [];
    private Activity? activity;
    private BusyState? busy;
    private string? errorMessage;
    private BinaryChoice? pendingChoice;
    private StopQuestion? pendingStop;
    private DateTimeOffset? lastChecked;

    /// <param name="core">The core to drive.</param>
    /// <param name="post">Runs an action on the UI thread; the default runs it where it is called.</param>
    public KetchStore(IKetchCore core, Action<Action>? post = null)
    {
        this.core = core;
        this.post = post ?? (action => action());
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string Root => core.Root;

    public IReadOnlyList<InstalledPackage> Installed => installed;

    public IReadOnlyList<Upgrade> Outdated => outdated;

    public IReadOnlyList<RegistryPackage> SearchResults => searchResults;

    public IReadOnlyList<Finding> Findings => findings;

    public IReadOnlyList<LogEntry> Log => log;

    public Activity? Activity => activity;

    public BusyState? Busy => busy;

    public string? ErrorMessage
    {
        get => errorMessage;
        set => Set(ref errorMessage, value);
    }

    public BinaryChoice? PendingChoice => pendingChoice;

    public StopQuestion? PendingStop => pendingStop;

    public DateTimeOffset? LastChecked => lastChecked;

    public bool IsRunning => activity is not null;

    /// <summary>Upgrades that `upgrade` would apply.</summary>
    public IReadOnlyList<Upgrade> Updates => [.. outdated.Where(u => u.HeldBy is null)];

    /// <summary>Upgrades a project's lockfile holds back.</summary>
    public IReadOnlyList<Upgrade> Held => [.. outdated.Where(u => u.HeldBy is not null)];

    public int PendingUpgradeCount => Updates.Count;

    public int ProblemCount => findings.Count(f => f.Severity != Severity.Ok);

    /// <summary>The version a package could be upgraded to, or null when it is current or held.</summary>
    public string? OutdatedVersion(string name) =>
        outdated.FirstOrDefault(u => u.Name == name && u.HeldBy is null)?.To;

    // Reading

    /// <summary>Re-reads installed and outdated packages; called on launch and after every operation.</summary>
    public async Task RefreshAsync()
    {
        try
        {
            var (now, upgrades) = await Background(c => (c.Installed(), c.Outdated()));
            installed = now;
            outdated = upgrades;
            lastChecked = DateTimeOffset.Now;
            Changed(nameof(Installed), nameof(Outdated), nameof(Updates), nameof(Held), nameof(PendingUpgradeCount), nameof(LastChecked));
        }
        catch (Exception error)
        {
            Report(error, RefreshAsync);
        }
    }

    public async Task SearchAsync(string query)
    {
        try
        {
            searchResults = await Background(c => c.Search(query));
            Changed(nameof(SearchResults));
        }
        catch (Exception error)
        {
            Report(error, null);
        }
    }

    /// <summary>The Markdown between two versions, or null when the core could not read it.</summary>
    public async Task<string?> ChangelogAsync(string name, string? from, string? to)
    {
        try
        {
            return await Background(c => c.Changelog(name, from, to));
        }
        catch (Exception error)
        {
            Report(error, null);
            return null;
        }
    }

    public async Task RunDoctorAsync()
    {
        try
        {
            findings = await Background(c => c.Doctor());
            Changed(nameof(Findings), nameof(ProblemCount));
        }
        catch (Exception error)
        {
            Report(error, RunDoctorAsync);
        }
    }

    // Operations

    public Task InstallAsync(string spec, bool includePrereleases = false) =>
        Operate(
            $"Installing {spec}",
            () => InstallAsync(spec, includePrereleases),
            (c, reporter, decider, cancel) => c.Install(spec, new InstallOptions(includePrereleases), reporter, decider, cancel));

    /// <summary>Upgrades <paramref name="names"/>, or everything outdated when empty.</summary>
    public Task UpgradeAsync(IReadOnlyList<string>? names = null)
    {
        names ??= [];
        var title = names.Count == 0 ? "Upgrading all packages" : $"Upgrading {string.Join(", ", names)}";
        return Operate(
            title,
            () => UpgradeAsync(names),
            (c, reporter, decider, cancel) => c.Upgrade(names, reporter, decider, cancel));
    }

    public Task UninstallAsync(IReadOnlyList<string> names) =>
        Operate(
            $"Uninstalling {string.Join(", ", names)}",
            () => UninstallAsync(names),
            (c, reporter, _, cancel) => c.Uninstall(names, reporter, cancel));

    /// <summary>
    /// Asks the running operation to stop at its next step, and declines any
    /// open question so the core is not left waiting on it.
    /// </summary>
    public void Cancel()
    {
        if (cancelToken is not { } token)
        {
            return;
        }

        if (activity is { } running)
        {
            running.IsCancelling = true;
            Changed(nameof(Activity));
        }

        token.Cancel();
        AnswerChoice(null);
        AnswerStop(false);
    }

    /// <summary>Re-runs what the held lock refused. The banner goes now and comes back if the lock is still held.</summary>
    public async Task RetryBusyAsync()
    {
        if (busy is not { } state)
        {
            return;
        }

        busy = null;
        Changed(nameof(Busy));
        await state.Retry();
    }

    public void AnswerChoice(int? choice)
    {
        if (pendingChoice is not { } pending)
        {
            return;
        }

        pendingChoice = null;
        Changed(nameof(PendingChoice));
        pending.Answer(choice);
    }

    public void AnswerStop(bool stop)
    {
        if (pendingStop is not { } pending)
        {
            return;
        }

        pendingStop = null;
        Changed(nameof(PendingStop));
        pending.Answer(stop);
    }

    public void DismissError() => ErrorMessage = null;

    // Internals

    private async Task Operate(
        string title, Func<Task> retry, Action<IKetchCore, IReporter, IDecider, CancelToken> body)
    {
        if (activity is not null)
        {
            return;
        }

        var token = new CancelToken();
        cancelToken = token;
        activity = new Activity(title);
        busy = null;
        Append(LogLevel.Info, title);
        Changed(nameof(Activity), nameof(IsRunning), nameof(Busy));
        try
        {
            await Background(c =>
            {
                body(c, new Reporter(this), new Decider(this, token), token);
                return 0;
            });
            Append(LogLevel.Success, $"Done: {title}");
        }
        catch (Exception error)
        {
            Report(error, retry);
        }

        cancelToken = null;
        activity = null;
        pendingChoice = null;
        pendingStop = null;
        Changed(nameof(Activity), nameof(IsRunning), nameof(PendingChoice), nameof(PendingStop));
        await RefreshAsync();
    }

    /// <summary>
    /// Runs a blocking core call on its own thread, not a pool worker: one that
    /// waits on the network or on a decider would starve the pool.
    /// </summary>
    private Task<T> Background<T>(Func<IKetchCore, T> work) =>
        Task.Factory.StartNew(() => work(core), CancellationToken.None, TaskCreationOptions.LongRunning, TaskScheduler.Default);

    private void Report(Exception error, Func<Task>? retry)
    {
        switch (error)
        {
            case KetchException { Kind: KetchErrorKind.Busy } busyError:
                Append(LogLevel.Warning, busyError.Message);
                if (retry is not null)
                {
                    busy = new BusyState(busyError.Pid, retry);
                    Changed(nameof(Busy));
                }

                break;
            case KetchException { Kind: KetchErrorKind.Cancelled }:
                Append(LogLevel.Warning, "Cancelled");
                break;
            default:
                Append(LogLevel.Error, error.Message);
                ErrorMessage = error.Message;
                break;
        }
    }

    private void Apply(CoreEvent coreEvent)
    {
        switch (coreEvent)
        {
            case CoreEvent.Step step:
                Progress(step.Package, step.Stage).Stage = step.Stage;
                Append(LogLevel.Info, $"{step.Package}: {step.Stage.ToString().ToLowerInvariant()}");
                break;
            case CoreEvent.Progress progress:
                var entry = Progress(progress.Package, Stage.Download);
                entry.Done = progress.Done;
                entry.Total = progress.Total;
                break;
            case CoreEvent.Status status:
                if (activity is { } current)
                {
                    current.Status = status.Message;
                }

                Append(LogLevel.Info, status.Message);
                break;
            case CoreEvent.Warning warning:
                Append(LogLevel.Warning, warning.Message);
                break;
        }

        Changed(nameof(Activity));
    }

    private PackageProgress Progress(string package, Stage stage)
    {
        var running = activity ?? new Activity("");
        if (!running.Packages.TryGetValue(package, out var progress))
        {
            progress = new PackageProgress(stage);
            running.Packages[package] = progress;
        }

        return progress;
    }

    private void Ask(BinaryChoice choice, CancelToken cancel)
    {
        if (cancel.IsCancelled)
        {
            choice.Answer(null);
            return;
        }

        pendingChoice?.Answer(null);
        pendingChoice = choice;
        Changed(nameof(PendingChoice));
    }

    private void Ask(StopQuestion question, CancelToken cancel)
    {
        if (cancel.IsCancelled)
        {
            question.Answer(false);
            return;
        }

        pendingStop?.Answer(false);
        pendingStop = question;
        Changed(nameof(PendingStop));
    }

    private void Append(LogLevel level, string message)
    {
        var entries = new List<LogEntry>(log) { new(++nextLogId, DateTimeOffset.Now, level, message) };
        if (entries.Count > LogLimit)
        {
            entries.RemoveRange(0, entries.Count - LogLimit);
        }

        log = entries;
        Changed(nameof(Log));
    }

    private void Set<T>(ref T field, T value, [CallerMemberName] string? name = null)
    {
        if (EqualityComparer<T>.Default.Equals(field, value))
        {
            return;
        }

        field = value;
        Changed(name!);
    }

    private void Changed(params string[] names)
    {
        foreach (var name in names)
        {
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
        }
    }

    /// <summary>Forwards core events to the store in the order the core sent them.</summary>
    private sealed class Reporter(KetchStore store) : IReporter
    {
        public void Event(CoreEvent coreEvent) => store.post(() => store.Apply(coreEvent));
    }

    /// <summary>
    /// Turns the core's blocking questions into dialogs: the core's thread
    /// waits on a gate while the UI thread shows the question.
    /// </summary>
    private sealed class Decider(KetchStore store, CancelToken cancel) : IDecider
    {
        public int? ChooseBinary(string package, IReadOnlyList<string> candidates)
        {
            if (cancel.IsCancelled)
            {
                return null;
            }

            using var done = new ManualResetEventSlim();
            int? answer = null;
            store.post(() => store.Ask(
                new BinaryChoice(package, candidates, choice =>
                {
                    answer = choice;
                    done.Set();
                }),
                cancel));
            done.Wait();
            return answer;
        }

        public bool StopProcesses(IReadOnlyList<ProcessHolder> holders)
        {
            if (cancel.IsCancelled)
            {
                return false;
            }

            using var done = new ManualResetEventSlim();
            var answer = false;
            store.post(() => store.Ask(
                new StopQuestion(holders, stop =>
                {
                    answer = stop;
                    done.Set();
                }),
                cancel));
            done.Wait();
            return answer;
        }
    }
}
