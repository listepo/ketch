// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The app's view of ketch's core: the operations, records, callbacks and
// errors `ketch-ffi` exports, as the Windows app wants them.
//
// Pages and the store only ever see `IKetchCore`, so they run against
// `FakeKetchCore` now and against the binding's adapter when D12 wires it;
// nothing here moves when that happens.

namespace Ketch.AppCore;

/// <summary>A package installed under the ketch root.</summary>
public sealed record InstalledPackage(
    string Name,
    string Version,
    string Source,
    string? Repo,
    string? Description,
    string Path);

/// <summary>A package the registry knows about.</summary>
public sealed record RegistryPackage(string Name, string Repo, string? Description, string? Latest);

/// <summary>An installed package with a newer release.</summary>
/// <param name="HeldBy">
/// The `ketch.lock` that pins the package at <paramref name="From"/>, when one
/// does; `upgrade` leaves a held package where it is.
/// </param>
public sealed record Upgrade(string Name, string From, string To, string? HeldBy);

public enum Severity { Ok, Warning, Error }

/// <summary>One `ketch doctor` check; <paramref name="Fix"/> labels the button for the fix the core offers.</summary>
public sealed record Finding(string Id, Severity Severity, string Message, string? Fix);

public sealed record InstallOptions(bool IncludePrereleases = false, bool Force = false);

/// <summary>A pipeline stage a package goes through, in order.</summary>
public enum Stage { Resolve, Download, Verify, Extract, Link, Hooks }

/// <summary>What the core reports while it works.</summary>
public abstract record CoreEvent
{
    public sealed record Step(string Package, Stage Stage) : CoreEvent;

    public sealed record Progress(string Package, ulong Done, ulong? Total) : CoreEvent;

    public sealed record Status(string Message) : CoreEvent;

    public sealed record Warning(string Message) : CoreEvent;
}

/// <summary>A process that holds a file the operation needs to replace.</summary>
public sealed record ProcessHolder(uint Pid, string Path);

/// <summary>Receives events from a running operation, on the core's thread.</summary>
public interface IReporter
{
    void Event(CoreEvent coreEvent);
}

/// <summary>
/// Answers the questions the core asks mid-operation, on the core's thread;
/// the core blocks until each returns.
/// </summary>
public interface IDecider
{
    /// <summary>Which of <paramref name="candidates"/> to link for <paramref name="package"/>; null declines.</summary>
    int? ChooseBinary(string package, IReadOnlyList<string> candidates);

    /// <summary>Whether to stop <paramref name="holders"/> so their files can be replaced.</summary>
    bool StopProcesses(IReadOnlyList<ProcessHolder> holders);
}

public enum KetchErrorKind { Busy, Cancelled, NotFound, Network, Verification, Other }

/// <summary>The core's errors, mirroring `KetchError`.</summary>
public sealed class KetchException : Exception
{
    public KetchException(KetchErrorKind kind, string message, uint? pid = null)
        : base(message)
    {
        Kind = kind;
        Pid = pid;
    }

    public KetchErrorKind Kind { get; }

    /// <summary>The process holding the lock; null when the core could not tell.</summary>
    public uint? Pid { get; }

    public static KetchException Busy(uint? pid) =>
        new(KetchErrorKind.Busy, pid is { } p ? $"ketch is running in another process (pid {p})." : "ketch is running in another process.", pid);

    public static KetchException Cancelled() => new(KetchErrorKind.Cancelled, "Cancelled.");

    public static KetchException NotFound(string name) => new(KetchErrorKind.NotFound, $"No package named {name}.");

    public static KetchException Network(string message) => new(KetchErrorKind.Network, $"Network error: {message}");

    public static KetchException Verification(string message) =>
        new(KetchErrorKind.Verification, $"Verification failed: {message}");

    public static KetchException Other(string message) => new(KetchErrorKind.Other, message);
}

/// <summary>
/// Cancels a running operation. The core polls <see cref="IsCancelled"/>
/// between steps; <see cref="OnCancel"/> lets an adapter forward the request to
/// the FFI token.
/// </summary>
public sealed class CancelToken
{
    private readonly object gate = new();
    private readonly List<Action> handlers = [];
    private bool cancelled;

    public bool IsCancelled
    {
        get
        {
            lock (gate)
            {
                return cancelled;
            }
        }
    }

    public void Cancel()
    {
        List<Action> run;
        lock (gate)
        {
            if (cancelled)
            {
                return;
            }

            cancelled = true;
            run = [.. handlers];
            handlers.Clear();
        }

        foreach (var handler in run)
        {
            handler();
        }
    }

    /// <summary>Runs <paramref name="handler"/> once when the token is cancelled, or now if it already is.</summary>
    public void OnCancel(Action handler)
    {
        bool now;
        lock (gate)
        {
            now = cancelled;
            if (!now)
            {
                handlers.Add(handler);
            }
        }

        if (now)
        {
            handler();
        }
    }
}

/// <summary>
/// ketch's core as the app uses it. Every method is synchronous and may block
/// for a long time, so callers run it off the UI thread.
/// </summary>
public interface IKetchCore
{
    /// <summary>The ketch root this core manages.</summary>
    string Root { get; }

    IReadOnlyList<InstalledPackage> Installed();

    IReadOnlyList<RegistryPackage> Search(string query);

    IReadOnlyList<Upgrade> Outdated();

    void Install(string spec, InstallOptions options, IReporter reporter, IDecider decider, CancelToken cancel);

    /// <summary>Upgrades <paramref name="names"/>, or everything outdated when it is empty.</summary>
    void Upgrade(IReadOnlyList<string> names, IReporter reporter, IDecider decider, CancelToken cancel);

    void Uninstall(IReadOnlyList<string> names, IReporter reporter, CancelToken cancel);

    /// <summary>The changelog between two versions as Markdown, already sanitized by the core.</summary>
    string Changelog(string name, string? from, string? to);

    IReadOnlyList<Finding> Doctor();
}
