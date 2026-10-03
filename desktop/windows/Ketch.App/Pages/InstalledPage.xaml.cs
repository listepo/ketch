// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// Installed: what is under the ketch root, filterable, with the update or hold
// each package carries and a confirmed uninstall.

using System.ComponentModel;
using Ketch.AppCore;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace KetchApp.Pages;

public sealed partial class InstalledPage : Page
{
    public InstalledPage()
    {
        InitializeComponent();
        Loaded += (_, _) =>
        {
            App.Store.PropertyChanged += OnStoreChanged;
            Render();
        };
        Unloaded += (_, _) => App.Store.PropertyChanged -= OnStoreChanged;
    }

    private void OnStoreChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(KetchStore.Installed) or nameof(KetchStore.Outdated))
        {
            Render();
        }
    }

    private void OnFilterChanged(object sender, TextChangedEventArgs e) => Render();

    private void Render()
    {
        var store = App.Store;
        var needle = Filter.Text.Trim();
        var rows = store.Installed
            .Where(p => needle.Length == 0 || p.Name.Contains(needle, StringComparison.OrdinalIgnoreCase))
            .Select(p => new InstalledRow(
                p,
                store.OutdatedVersion(p.Name),
                store.Held.FirstOrDefault(u => u.Name == p.Name)?.HeldBy))
            .ToList();
        List.ItemsSource = rows;
        Empty.Visibility = Fmt.ShowWhenEmpty(rows.Count);
        List.Visibility = Fmt.ShowWhenAny(rows.Count);
        EmptyText.Text = needle.Length == 0 ? "Nothing is installed yet" : $"No installed package matches \"{needle}\"";
    }

    private void OnOpen(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is InstalledRow row)
        {
            App.Main?.OpenPackage(row.Name);
        }
    }

    private async void OnUninstall(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { Tag: string name })
        {
            return;
        }

        if (await Dialogs.ConfirmAsync(
                this, $"Uninstall {name}?", $"{name} and its links are removed from the ketch root.", "Uninstall"))
        {
            await App.Store.UninstallAsync([name]);
        }
    }
}
