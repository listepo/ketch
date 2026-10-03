// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The ContentDialogs the app asks decisions with: the core's questions and the
// confirmations before something is removed or changed. Each returns the
// answer; none touches the store, so a page decides what a "yes" runs.

using Ketch.AppCore;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace KetchApp;

internal static class Dialogs
{
    /// <summary>Which of the binaries sharing a package's name to link; null when declined.</summary>
    public static async Task<int?> ChooseBinaryAsync(FrameworkElement anchor, BinaryChoice choice)
    {
        var options = new RadioButtons { ItemsSource = choice.Candidates, SelectedIndex = 0 };
        var content = new StackPanel
        {
            Spacing = 12,
            Children =
            {
                new TextBlock
                {
                    Text = $"{choice.Package} ships several programs. ketch links one onto your PATH.",
                    TextWrapping = TextWrapping.Wrap,
                },
                options,
            },
        };
        var dialog = Create(anchor, $"Which binary should {choice.Package} link?", content, "Link", "Cancel");
        return await dialog.ShowAsync() == ContentDialogResult.Primary ? options.SelectedIndex : null;
    }

    /// <summary>Whether to stop the processes that hold files the upgrade replaces.</summary>
    public static async Task<bool> StopProcessesAsync(FrameworkElement anchor, StopQuestion question)
    {
        var list = string.Join('\n', question.Holders.Select(h => $"{h.Path} (pid {h.Pid})"));
        var dialog = Create(
            anchor,
            "Stop running programs?",
            new TextBlock
            {
                Text = $"These programs use files the upgrade replaces:\n\n{list}",
                TextWrapping = TextWrapping.Wrap,
                IsTextSelectionEnabled = true,
            },
            "Stop them",
            "Cancel");
        return await dialog.ShowAsync() == ContentDialogResult.Primary;
    }

    public static async Task<bool> ConfirmAsync(
        FrameworkElement anchor, string title, string message, string confirm)
    {
        var dialog = Create(
            anchor, title, new TextBlock { Text = message, TextWrapping = TextWrapping.Wrap }, confirm, "Cancel");
        return await dialog.ShowAsync() == ContentDialogResult.Primary;
    }

    private static ContentDialog Create(
        FrameworkElement anchor, string title, object content, string primary, string close) => new()
    {
        // A dialog is parented to the window's XamlRoot, and takes the theme
        // the window shows, so it follows the user's choice and high contrast.
        XamlRoot = anchor.XamlRoot,
        RequestedTheme = anchor.ActualTheme,
        Title = title,
        Content = content,
        PrimaryButtonText = primary,
        CloseButtonText = close,
        DefaultButton = ContentDialogButton.Primary,
    };
}
