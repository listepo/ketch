// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// A package's page: overview, the changelog between the installed and the
// latest version, and the actions that apply to it.

using System.ComponentModel;
using Ketch.AppCore;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;

namespace KetchApp.Pages;

public sealed partial class PackageDetailPage : Page
{
    private string name = "";

    public PackageDetailPage()
    {
        InitializeComponent();
        Unloaded += (_, _) => App.Store.PropertyChanged -= OnStoreChanged;
    }

    protected override async void OnNavigatedTo(NavigationEventArgs e)
    {
        name = e.Parameter as string ?? "";
        App.Store.PropertyChanged += OnStoreChanged;
        Render();
        var installed = App.Store.Installed.FirstOrDefault(p => p.Name == name);
        ChangelogText.Text = "Loading…";
        ChangelogText.Text = await App.Store.ChangelogAsync(name, installed?.Version, App.Store.OutdatedVersion(name))
            ?? "The changelog could not be read.";
    }

    private void OnStoreChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(KetchStore.Installed) or nameof(KetchStore.Outdated) or nameof(KetchStore.Activity))
        {
            Render();
        }
    }

    private void Render()
    {
        var store = App.Store;
        NameText.Text = name;
        var package = store.Installed.FirstOrDefault(p => p.Name == name);
        DescriptionText.Text = package?.Description ?? "";
        VersionText.Text = package?.Version ?? "Not installed";
        SourceText.Text = package?.Source ?? "";
        RepoText.Text = package?.Repo ?? "";
        PathText.Text = package?.Path ?? "";

        var update = store.OutdatedVersion(name);
        UpgradeButton.Content = update is null ? "Up to date" : $"Update to {update}";
        UpgradeButton.IsEnabled = update is not null && !store.IsRunning;
        UninstallButton.IsEnabled = package is not null && !store.IsRunning;
    }

    private void OnBack(object sender, RoutedEventArgs e)
    {
        if (Frame.CanGoBack)
        {
            Frame.GoBack();
        }
    }

    private async void OnUpgrade(object sender, RoutedEventArgs e) => await App.Store.UpgradeAsync([name]);

    private async void OnUninstall(object sender, RoutedEventArgs e)
    {
        if (await Dialogs.ConfirmAsync(
                this, $"Uninstall {name}?", $"{name} and its links are removed from the ketch root.", "Uninstall"))
        {
            await App.Store.UninstallAsync([name]);
            if (Frame.CanGoBack)
            {
                Frame.GoBack();
            }
        }
    }
}
