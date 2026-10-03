// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The contract scenarios in `desktop/contract/scenarios/`, as C# values.
//
// They are generated from `ketch-ffi`'s Rust types (`crates/ketch-ffi/tests/
// contract.rs`) and read by every app's fake core, so the fake speaks the
// records, event streams and errors the real core does. This file only
// decodes them and maps the wire shape onto the app's own types; the replay
// itself lives in `FakeKetchCore`. It mirrors `ContractScenario.swift`, and
// the mapping is the same one the live adapter will make: stages are coarser,
// progress is keyed by task id, a missing lock holder has no pid.

using System.Text.Json;

namespace Ketch.AppCore;

public sealed class ContractScenario
{
    private static readonly JsonSerializerOptions Json = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        PropertyNameCaseInsensitive = true,
    };

    public required string Name { get; init; }

    public required string Description { get; init; }

    public required CallInfo Call { get; init; }

    public required IReadOnlyList<ScriptStep> Script { get; init; }

    public required Outcome Outcome { get; init; }

    /// <summary>The returned value as <typeparamref name="T"/>; throws the scenario's error when it failed instead.</summary>
    public T Value<T>()
    {
        if (Outcome.Error is { } error)
        {
            throw error.AsException();
        }

        return Outcome.Value.Deserialize<T>(Json)
            ?? throw new InvalidDataException($"scenario {Name} has no value");
    }

    /// <summary>Every scenario in <paramref name="directory"/>, sorted by file name.</summary>
    public static IReadOnlyList<ContractScenario> Load(string directory) =>
        [.. Directory.EnumerateFiles(directory, "*.json")
            .Order(StringComparer.Ordinal)
            .Select(file => Parse(File.ReadAllText(file)))];

    public static ContractScenario Parse(string json) =>
        JsonSerializer.Deserialize<ContractScenario>(json, Json)
            ?? throw new InvalidDataException("empty scenario");

    /// <summary>The scenarios that ship beside the app (see Ketch.AppCore.csproj).</summary>
    public static string DefaultDirectory => Path.Combine(AppContext.BaseDirectory, "scenarios");

    /// <summary>`github:owner/repo` as ("github", "owner/repo").</summary>
    public static (string Scheme, string Id) SplitSource(string source)
    {
        var colon = source.IndexOf(':');
        return colon < 0 ? (source, "") : (source[..colon], source[(colon + 1)..]);
    }

    /// <summary>Changelog sections as the Markdown the app renders, newest first.</summary>
    public static string Markdown(IEnumerable<ChangelogEntry> entries) =>
        string.Join("\n\n", entries.Select(entry => $"## {entry.Heading ?? entry.Version}\n\n{entry.Body}"));

    public sealed class CallInfo
    {
        public required string Operation { get; init; }

        public IReadOnlyList<string>? Specs { get; init; }

        public IReadOnlyList<string>? Names { get; init; }

        public string? Package { get; init; }

        public string? Query { get; init; }
    }

    public sealed class Package
    {
        public required string Name { get; init; }

        public required string Version { get; init; }

        public required string Source { get; init; }

        public required string Prefix { get; init; }

        public InstalledPackage AsInstalled()
        {
            var (scheme, id) = SplitSource(Source);
            return new InstalledPackage(Name, Version, scheme, scheme == "github" ? id : null, null, Prefix);
        }
    }

    public sealed class Installed
    {
        public required Package Package { get; init; }

        public string? Replaced { get; init; }
    }

    public sealed class RegistryEntry
    {
        public required string Name { get; init; }

        public required string Source { get; init; }

        public string? Description { get; init; }

        public string? Latest { get; init; }

        public RegistryPackage AsRegistryPackage()
        {
            var (scheme, id) = SplitSource(Source);
            return new RegistryPackage(Name, scheme == "github" ? id : Source, Description, Latest);
        }
    }

    public sealed class SearchResults
    {
        public required IReadOnlyList<RegistryEntry> Known { get; init; }
    }

    public sealed class UpgradeEntry
    {
        public required string Name { get; init; }

        public required string Installed { get; init; }

        public required string Latest { get; init; }

        public bool Pinned { get; init; }

        public string? HeldBy { get; init; }

        public Upgrade AsUpgrade() => new(Name, Installed, Latest, Pinned ? HeldBy ?? "a pin" : null);
    }

    public sealed class ChangelogEntry
    {
        public required string Version { get; init; }

        public string? Heading { get; init; }

        public required string Body { get; init; }
    }

    public sealed class CheckEntry
    {
        public required string Name { get; init; }

        public required string Outcome { get; init; }

        public required string Detail { get; init; }

        public string? Fix { get; init; }

        public Finding AsFinding() => new(
            Name,
            Outcome switch { "ok" => Severity.Ok, "warn" => Severity.Warning, _ => Severity.Error },
            Detail,
            Fix);
    }

    public sealed class TaskKind
    {
        public required string Type { get; init; }

        public string? Label { get; init; }
    }

    /// <summary>Every field of every event kind, optional: the type says which are set.</summary>
    public sealed class Event
    {
        public required string Type { get; init; }

        public string? Package { get; init; }

        public string? Stage { get; init; }

        public string? Verb { get; init; }

        public string? Detail { get; init; }

        public ulong? Id { get; init; }

        public TaskKind? Task { get; init; }

        public ulong? Done { get; init; }

        public ulong? Total { get; init; }
    }

    public sealed class Holder
    {
        public uint Pid { get; init; }

        public string Path { get; init; } = "";
    }

    public sealed class Question
    {
        public required string Type { get; init; }

        public string? Package { get; init; }

        public IReadOnlyList<string>? Candidates { get; init; }

        public IReadOnlyList<Holder>? Holders { get; init; }
    }

    /// <summary>One step of a script: an event the core reports, or a question it asks.</summary>
    public sealed class ScriptStep
    {
        public required string Type { get; init; }

        public Event? Event { get; init; }

        public Question? Question { get; init; }
    }

    public sealed class WireError
    {
        public required string Type { get; init; }

        public uint? Pid { get; init; }

        public string? Name { get; init; }

        public string? Message { get; init; }

        public KetchException AsException() => Type switch
        {
            "busy" => KetchException.Busy(Pid),
            "cancelled" => KetchException.Cancelled(),
            "not_found" => KetchException.NotFound(Name ?? ""),
            "network" => KetchException.Network(Message ?? ""),
            "verification" => KetchException.Verification(Message ?? ""),
            _ => KetchException.Other(Message ?? ""),
        };
    }
}

public sealed class Outcome
{
    public required string Type { get; init; }

    public JsonElement Value { get; init; }

    public ContractScenario.WireError? Error { get; init; }
}

/// <summary>
/// Turns the core's events into the app's, remembering which package a task id
/// downloads for, because the app's progress is keyed by package.
/// </summary>
public sealed class ContractEventMapper
{
    private readonly Dictionary<ulong, string> labels = [];

    public IReadOnlyList<CoreEvent> Map(ContractScenario.Event e)
    {
        switch (e.Type)
        {
            case "step" when e.Package is { } package && ToStage(e.Stage) is { } stage:
                return [new CoreEvent.Step(package, stage)];
            case "status" or "success":
                return [new CoreEvent.Status(string.Join(' ', new[] { e.Verb, e.Detail }.OfType<string>()))];
            case "warn":
                return [new CoreEvent.Warning(e.Detail ?? "")];
            case "note":
                return [new CoreEvent.Status(e.Detail ?? "")];
            case "began":
                if (e.Id is { } began && e.Task?.Label is { } label)
                {
                    labels[began] = label;
                }

                return [];
            case "progress" when e.Id is { } id && labels.TryGetValue(id, out var task):
                return [new CoreEvent.Progress(task, e.Done ?? 0, e.Total)];
            case "ended" or "abandoned":
                // The app's events have no end of a task: the stage moving on
                // takes a bar down, and a dropped download is followed by the
                // error that stopped it.
                if (e.Id is { } ended)
                {
                    labels.Remove(ended);
                }

                return [];
            default:
                return [];
        }
    }

    /// <summary>The core's stages are finer than the app's: trusting is part of verifying, and installing is placing the links.</summary>
    private static Stage? ToStage(string? name) => name switch
    {
        "resolving" => Stage.Resolve,
        "downloading" => Stage.Download,
        "verifying" or "trusting" => Stage.Verify,
        "extracting" => Stage.Extract,
        "installing" => Stage.Link,
        _ => null,
    };
}
