// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// Small formatting helpers for x:Bind function bindings; pure, so they stay
// out of the pages.

using Ketch.AppCore;
using Microsoft.UI.Xaml;

namespace KetchApp;

/// <summary>An installed package as a row: with the version it could move to, and the lockfile holding it back.</summary>
public sealed record InstalledRow(InstalledPackage Package, string? Update, string? HeldBy)
{
    public string Name => Package.Name;

    public string Version => Package.Version;

    public string Summary => Package.Description ?? Package.Repo ?? Package.Source;

    public string UpdateText => Update is { } to ? $"Update to {to}" : "";

    public bool HasUpdate => Update is not null;

    public bool IsHeld => HeldBy is not null && Update is null;
}

internal static class Fmt
{
    public static Visibility Show(bool visible) => visible ? Visibility.Visible : Visibility.Collapsed;

    public static Visibility ShowWhenEmpty(int count) => count == 0 ? Visibility.Visible : Visibility.Collapsed;

    public static Visibility ShowWhenAny(int count) => count > 0 ? Visibility.Visible : Visibility.Collapsed;

    public static Visibility ShowText(string? text) => Show(!string.IsNullOrEmpty(text));

    public static string Change(string from, string to) => $"{from} → {to}";

    public static string Held(string? lockfile) => $"Held by {lockfile}";

    public static string Latest(string? latest) => latest is null ? "" : $"Latest {latest}";

    public static string Plural(int count, string noun) => count == 1 ? $"1 {noun}" : $"{count} {noun}s";

    /// <summary>A Segoe Fluent Icons glyph for a doctor finding.</summary>
    public static string Glyph(Severity severity) => severity switch
    {
        Severity.Ok => "",
        Severity.Warning => "",
        _ => "",
    };

    public static string Time(DateTimeOffset at) => at.ToString("HH:mm:ss");
}
