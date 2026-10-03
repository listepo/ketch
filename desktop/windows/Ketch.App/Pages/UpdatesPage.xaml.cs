// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// Updates: what `upgrade` would apply, with Update all behind a confirmation,
// and the packages a lockfile holds back in their own group.

using System.ComponentModel;
using Ketch.AppCore;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace KetchApp.Pages;

public sealed partial class UpdatesPage : Page
{
    public UpdatesPage()
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
        if (e.PropertyName is nameof(KetchStore.Outdated) or nameof(KetchStore.LastChecked) or nameof(KetchStore.Activity))
        {
            Render();
        }
    }

    private void Render()
    {
        var store = App.Store;
        var updates = store.Updates;
        Pending.ItemsSource = updates;
        Pending.Visibility = Fmt.ShowWhenAny(updates.Count);
        UpToDate.Visibility = Fmt.ShowWhenEmpty(updates.Count);
        Held.ItemsSource = store.Held;
        HeldGroup.Visibility = Fmt.ShowWhenAny(store.Held.Count);
        UpdateAll.IsEnabled = updates.Count > 0 && !store.IsRunning;
        Check.IsEnabled = !store.IsRunning;
        Checked.Text = store.LastChecked is { } at ? $"Checked at {at:HH:mm}" : "Not checked yet";
    }

    private async void OnCheck(object sender, RoutedEventArgs e) => await App.Store.RefreshAsync();

    private async void OnUpdate(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string name })
        {
            await App.Store.UpgradeAsync([name]);
        }
    }

    private async void OnUpdateAll(object sender, RoutedEventArgs e)
    {
        var count = App.Store.PendingUpgradeCount;
        if (await Dialogs.ConfirmAsync(
                this,
                $"Update {Fmt.Plural(count, "package")}?",
                "Each package is downloaded, verified and linked in turn. Held packages stay where they are.",
                "Update all"))
        {
            await App.Store.UpgradeAsync();
        }
    }
}
